use std::old_io;

use conduit;
use conduit::Request;
use semver;

pub struct RequestProxy<'a> {
    pub other: &'a mut (Request + 'a),
    pub path: Option<&'a str>,
    pub method: Option<conduit::Method>,
}

impl<'a> Request for RequestProxy<'a> {
    fn http_version(&self) -> semver::Version {
        self.other.http_version()
    }
    fn conduit_version(&self) -> semver::Version {
        self.other.conduit_version()
    }
    fn method(&self) -> conduit::Method {
        self.method.unwrap_or(self.other.method())
    }
    fn scheme(&self) -> conduit::Scheme { self.other.scheme() }
    fn host(&self) -> conduit::Host { self.other.host() }
    fn virtual_root(&self) -> Option<&str> {
        self.other.virtual_root()
    }
    fn path(&self) -> &str {
        self.path.map(|s| &*s).unwrap_or(self.other.path())
    }
    fn query_string(&self) -> Option<&str> {
        self.other.query_string()
    }
    fn remote_ip(&self) -> old_io::net::ip::IpAddr { self.other.remote_ip() }
    fn content_length(&self) -> Option<u64> {
        self.other.content_length()
    }
    fn headers(&self) -> &conduit::Headers {
        self.other.headers()
    }
    fn body(&mut self) -> &mut Reader { self.other.body() }
    fn extensions(&self) -> &conduit::Extensions {
        self.other.extensions()
    }
    fn mut_extensions(&mut self) -> &mut conduit::Extensions {
        self.other.mut_extensions()
    }
}
