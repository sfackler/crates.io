#![deny(warnings)]
#![feature(std_misc, core, os, io, env)]

extern crate "cargo-registry" as cargo_registry;
extern crate postgres;
extern crate semver;
extern crate time;
extern crate env_logger;

use std::env;
use std::collections::HashMap;
use std::time::Duration;

use cargo_registry::{VersionDownload, Version, Model};

static LIMIT: i64 = 10000;

#[allow(dead_code)] // dead in tests
fn main() {
    env_logger::init().unwrap();
    let daemon = env::args().nth(1).as_ref().map(|s| s.to_str().unwrap())
                    == Some("daemon");
    let sleep = env::args().nth(2).map(|s| s.to_str().unwrap().parse::<i64>().unwrap());
    loop {
        let conn = postgres::Connection::connect(env("DATABASE_URL").as_slice(),
                                                 &postgres::SslMode::None).unwrap();
        {
            let tx = conn.transaction().unwrap();
            update(&tx).unwrap();
            tx.set_commit();
            tx.finish().unwrap();
        }
        drop(conn);
        if daemon {
            std::old_io::timer::sleep(Duration::seconds(sleep.unwrap()));
        } else {
            break
        }
    }
}

fn env(s: &str) -> String {
    match env::var_string(s).ok() {
        Some(s) => s,
        None => panic!("must have `{}` defined", s),
    }
}

fn update(tx: &postgres::Transaction) -> postgres::Result<()> {
    let mut max = 0;
    loop {
        let tx = try!(tx.transaction());
        {
            let stmt = try!(tx.prepare("SELECT * FROM version_downloads \
                                        WHERE processed = FALSE AND id > $1
                                        ORDER BY id ASC
                                        LIMIT $2"));
            let mut rows = try!(stmt.query(&[&max, &LIMIT]));
            match try!(collect(&tx, &mut rows)) {
                None => break,
                Some(m) => max = m,
            }
        }
        tx.set_commit();
        try!(tx.finish());
    }
    Ok(())
}

fn collect(tx: &postgres::Transaction,
           rows: &mut postgres::Rows) -> postgres::Result<Option<i32>> {

    // Anything older than 24 hours ago will be frozen and will not be queried
    // against again.
    let cutoff = time::now_utc().to_timespec();
    let cutoff = cutoff + Duration::days(-1);

    let mut map = HashMap::new();
    for row in rows.by_ref() {
        let download: VersionDownload = Model::from_row(&row);
        assert!(map.insert(download.id, download).is_none());
    }
    println!("updating {} versions", map.len());
    if map.len() == 0 { return Ok(None) }

    let mut max = 0;
    let mut total = 0;
    for (id, download) in map.iter() {
        if *id > max { max = *id; }
        if download.counted == download.downloads { continue }
        let amt = download.downloads - download.counted;

        let crate_id = Version::find(tx, download.version_id).unwrap().crate_id;

        // Update the total number of version downloads
        try!(tx.execute("UPDATE versions
                         SET downloads = downloads + $1
                         WHERE id = $2",
                        &[&amt, &download.version_id]));
        // Update the total number of crate downloads
        try!(tx.execute("UPDATE crates SET downloads = downloads + $1
                         WHERE id = $2", &[&amt, &crate_id]));

        // Update the total number of crate downloads for today
        let cnt = try!(tx.execute("UPDATE crate_downloads
                                   SET downloads = downloads + $2
                                   WHERE crate_id = $1 AND date = date($3)",
                                  &[&crate_id, &amt, &download.date]));
        if cnt == 0 {
            try!(tx.execute("INSERT INTO crate_downloads
                             (crate_id, downloads, date)
                             VALUES ($1, $2, $3)",
                            &[&crate_id, &amt, &download.date]));
        }

        // Flag this row as having been processed if we're passed the cutoff,
        // and unconditionally increment the number of counted downloads.
        try!(tx.execute("UPDATE version_downloads
                         SET processed = $2, counted = downloads
                         WHERE id = $1",
                        &[id, &(download.date < cutoff)]));
        total += amt as i64;
    }

    // After everything else is done, update the global counter of total
    // downloads.
    try!(tx.execute("UPDATE metadata SET total_downloads = total_downloads + $1",
                    &[&total]));

    Ok(Some(max))
}

#[cfg(test)]
mod test {
    use std::collections::HashMap;

    use postgres;
    use semver;

    use cargo_registry::{Version, Crate, User};

    fn conn() -> postgres::Connection {
        postgres::Connection::connect(::env("TEST_DATABASE_URL").as_slice(),
                                      &postgres::SslMode::None).unwrap()
    }

    fn user(conn: &postgres::Transaction) -> User{
        User::find_or_insert(conn, "login", None, None, None,
                             "access_token", "api_token").unwrap()
    }

    fn crate_downloads(tx: &postgres::Transaction, id: i32, expected: usize) {
        let stmt = tx.prepare("SELECT * FROM crate_downloads
                               WHERE crate_id = $1").unwrap();
        let dl: i32 = stmt.query(&[&id]).unwrap()
                          .next().unwrap().get("downloads");
        assert_eq!(dl, expected as i32);
    }

    #[test]
    fn increment() {
        let conn = conn();
        let tx = conn.transaction().unwrap();
        let user = user(&tx);
        let krate = Crate::find_or_insert(&tx, "foo", user.id, &None, &None,
                                          &None, &None, &[], &None, &None,
                                          &None).unwrap();
        let version = Version::insert(&tx, krate.id,
                                      &semver::Version::parse("1.0.0").unwrap(),
                                      &HashMap::new(), &[]).unwrap();
        tx.execute("INSERT INTO version_downloads \
                    (version_id, downloads, counted, date, processed)
                    VALUES ($1, 1, 0, current_date, false)",
                   &[&version.id]).unwrap();
        tx.execute("INSERT INTO version_downloads \
                    (version_id, downloads, counted, date, processed)
                    VALUES ($1, 1, 0, current_date, true)",
                   &[&version.id]).unwrap();
        ::update(&tx).unwrap();
        assert_eq!(Version::find(&tx, version.id).unwrap().downloads, 1);
        assert_eq!(Crate::find(&tx, krate.id).unwrap().downloads, 1);
        crate_downloads(&tx, krate.id, 1);
        ::update(&tx).unwrap();
        assert_eq!(Version::find(&tx, version.id).unwrap().downloads, 1);
    }

    #[test]
    fn increment_a_little() {
        let conn = conn();
        let tx = conn.transaction().unwrap();
        let user = user(&tx);
        let krate = Crate::find_or_insert(&tx, "foo", user.id, &None,
                                          &None, &None, &None, &[], &None,
                                          &None, &None).unwrap();
        let version = Version::insert(&tx, krate.id,
                                      &semver::Version::parse("1.0.0").unwrap(),
                                      &HashMap::new(), &[]).unwrap();
        tx.execute("INSERT INTO version_downloads \
                    (version_id, downloads, counted, date, processed)
                    VALUES ($1, 2, 1, current_date, false)",
                   &[&version.id]).unwrap();
        tx.execute("INSERT INTO version_downloads \
                    (version_id, downloads, counted, date, processed)
                    VALUES ($1, 1, 0, current_date, false)",
                   &[&version.id]).unwrap();
        ::update(&tx).unwrap();
        assert_eq!(Version::find(&tx, version.id).unwrap().downloads, 2);
        assert_eq!(Crate::find(&tx, krate.id).unwrap().downloads, 2);
        crate_downloads(&tx, krate.id, 2);
        ::update(&tx).unwrap();
        assert_eq!(Version::find(&tx, version.id).unwrap().downloads, 2);
    }
}
