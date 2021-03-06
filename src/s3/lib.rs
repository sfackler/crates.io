#![deny(warnings)]
#![feature(core, io)]

extern crate time;
extern crate curl;
extern crate "rustc-serialize" as serialize;
extern crate openssl;

use curl::http;
use curl::http::body::ToBody;
use openssl::crypto::{hmac, hash};
use serialize::base64::{ToBase64, STANDARD};

pub struct Bucket {
    name: String,
    region: Option<String>,
    access_key: String,
    secret_key: String,
    proto: String,
}

impl Bucket {
    pub fn new(name: String,
               region: Option<String>,
               access_key: String,
               secret_key: String,
               proto: &str) -> Bucket {
        Bucket {
            name: name,
            region: region,
            access_key: access_key,
            secret_key: secret_key,
            proto: proto.to_string(),
        }
    }

    pub fn put<'a, 'b, T: ToBody<'b>>(&self, handle: &'a mut http::Handle,
                                      path: &str, content: T,
                                      content_type: &str)
                                      -> http::Request<'a, 'b> {
        let path = if path.starts_with("/") {&path[1..]} else {path};
        let host = self.host();
        let date = time::now().rfc822z().to_string();
        let auth = self.auth("PUT", date.as_slice(), path, "", content_type);
        let url = format!("{}://{}/{}", self.proto, host, path);
        handle.put(url.as_slice(), content)
              .header("Host", host.as_slice())
              .header("Date", date.as_slice())
              .header("Authorization", auth.as_slice())
              .content_type(content_type)
    }

    pub fn delete<'a, 'b>(&self, handle: &'a mut http::Handle, path: &str)
                          -> http::Request<'a, 'b> {
        let path = if path.starts_with("/") {&path[1..]} else {path};
        let host = self.host();
        let date = time::now().rfc822z().to_string();
        let auth = self.auth("DELETE", date.as_slice(), path, "", "");
        let url = format!("{}://{}/{}", self.proto, host, path);
        handle.delete(url.as_slice())
              .header("Host", host.as_slice())
              .header("Date", date.as_slice())
              .header("Authorization", auth.as_slice())
    }

    pub fn host(&self) -> String {
        format!("{}.s3{}.amazonaws.com", self.name,
                match self.region {
                    Some(ref r) => format!("-{}", r),
                    None => String::new(),
                })
    }

    fn auth(&self, verb: &str, date: &str, path: &str,
            md5: &str, content_type: &str) -> String {
        let string = format!("{verb}\n{md5}\n{ty}\n{date}\n{headers}{resource}",
                             verb = verb,
                             md5 = md5,
                             ty = content_type,
                             date = date,
                             headers = "",
                             resource = format!("/{}/{}", self.name, path));
        let signature = {
            let mut hmac = hmac::HMAC::new(hash::Type::SHA1, self.secret_key.as_bytes());
            let _ = hmac.write_all(string.as_bytes());
            hmac.finish().as_slice().to_base64(STANDARD)
        };
        format!("AWS {}:{}", self.access_key, signature)
    }
}
