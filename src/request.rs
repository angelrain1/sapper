use std::net::SocketAddr;
use std::io::Read;

use hyper::server::Request as HyperRequest;
use hyper::{Method, HttpVersion, Headers};
use hyper::Chunk;
use futures::{Stream, Future};
use typemap::TypeMap;
use std::mem::replace;

pub struct SapperRequest {
    raw_req: HyperRequest,
    ext: TypeMap
}

impl SapperRequest {
    pub fn new(req: HyperRequest) -> SapperRequest {
        SapperRequest {
            raw_req: req,
            ext: TypeMap::new()
        }
    }

    pub fn remote_addr(&self) -> SocketAddr {
        self.raw_req.remote_addr().expect("")
    }

    pub fn method(&self) -> &Method {
        &self.raw_req.method()
    }

    pub fn version(&self) -> HttpVersion {
        self.raw_req.version()
    }

    pub fn headers(&self) -> &Headers {
        &self.raw_req.headers()
    }

    // uri() -> (path, query)
    pub fn uri(&self) -> (&str, Option<&str>) {
        let uri = self.raw_req.uri();
        if uri.is_absolute() {
            (uri.path(), uri.query())
        } else {
            unreachable!()
        }
    }

    pub fn query(&self) -> Option<&str> {
        self.raw_req.query()
    }

    pub fn path(&self) -> &str {
        self.raw_req.path()
    }

    // here, we read it all for simplify upper business logic
    pub fn body(&mut self) -> Option<Vec<u8>> {
        if self.raw_req.body_ref().is_none() {
            return None;
        }

        //if body is not None, then replace the raw_req and get body
        let mut tmp = HyperRequest::new(self.raw_req.method().clone(), self.raw_req.uri().clone());
        tmp.set_version(self.raw_req.version());

        //here reset the headers
        let (m, u, v, h, b) = replace(&mut self.raw_req, tmp).deconstruct();
        (*self.raw_req.headers_mut()) = h;

        b.concat2().map(|chunk| { (&chunk).to_vec() }).wait()
            .map_err(|_| { println!("request body reading error!"); })
            .ok()
    }

    pub fn ext(&self) -> &TypeMap {
        &self.ext
    }

    pub fn ext_mut(&mut self) -> &mut TypeMap {
        &mut self.ext
    }
}

