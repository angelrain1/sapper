use std::str;
use std::io::Read;
use std::fs::File;
use std::path::Path;
use std::sync::Arc;
use std::clone::Clone;

use futures::future::{FutureResult, ok, err};
use hyper::StatusCode;
use hyper::server::{Server, Request, Response, Service, Http, const_service};
use hyper::header::ContentType;
use hyper::Error as HyperError;
use mime_types::Types as MimeTypes;
use tokio_core::reactor::Core;
use futures::future::empty;
use futures::{Stream, Future};
use num_cpus;

pub use typemap::Key;
pub use hyper::header::Headers;
pub use hyper::header;
pub use hyper::mime;
pub use request::SapperRequest;
pub use response::SapperResponse;
pub use router_m::Router;
pub use router::SapperRouter;
pub use handler::SapperHandler;


#[derive(Clone)]
pub struct PathParams;

/// Status Codes
pub mod status {
    pub use hyper::StatusCode as Status;
    pub use hyper::StatusCode::*;
}

#[derive(Debug, PartialEq, Clone)]
pub enum Error {
    NotFound(String),
    InvalidConfig,
    InvalidRouterConfig,
    FileNotExist,
    ShouldRedirect(String),
    Break(String),
    Fatal(String),
    Custom(String),
}


pub type Result<T> = ::std::result::Result<T, Error>;


pub trait SapperModule: Sync + Send {
    fn before(&self, req: &mut SapperRequest) -> Result<()> {
        Ok(())
    }

    fn after(&self, req: &SapperRequest, res: &mut SapperResponse) -> Result<()> {
        Ok(())
    }

    fn router(&self, _: &mut SapperRouter) -> Result<()>;
}

pub trait SapperAppShell {
    fn before(&self, _: &mut SapperRequest) -> Result<()>;
    fn after(&self, _: &SapperRequest, _: &mut SapperResponse) -> Result<()>;
}

pub type GlobalInitClosure = Box<Fn(&mut SapperRequest) -> Result<()> + 'static + Send + Sync>;
pub type SapperAppShellType = Box<SapperAppShell + 'static + Send + Sync>;

pub struct SapperApp {
    pub address: String,
    pub port: u32,
    // for app entry, global middeware
    pub shell: Option<Arc<SapperAppShellType>>,
    // for actually use to recognize
    pub routers: Router,
    // do simple static file service
    pub static_service: bool,

    pub init_closure: Option<Arc<GlobalInitClosure>>
}

use std::sync::mpsc::{channel, Sender, Receiver};
use std::thread::{spawn, JoinHandle};
use std::fmt::Debug;

fn run<E: Debug, F: Future<Error=E> + 'static + Send>(rx: Receiver<F>) {
    let mut core = Core::new().unwrap();
    let handle = core.handle();

    for conn in rx.iter() {
        handle.spawn(conn.map(|_| ()).map_err(|err| println!("srv1 error: {:?}", err)))
    }
    core.run(empty::<(), ()>()).unwrap();
}

fn worker<E: Debug, F: Future<Error=E> + 'static + Send>() -> Sender<F> {
    let (tx, rx) = channel();
    spawn(move || run(rx));
    tx
}

impl SapperApp {
    pub fn new() -> SapperApp {
        SapperApp {
            address: String::new(),
            port: 0,
            shell: None,
            routers: Router::new(),
            static_service: true,
            init_closure: None
        }
    }

    pub fn address(&mut self, address: &str) -> &mut Self {
        self.address = address.to_owned();
        self
    }

    pub fn port(&mut self, port: u32) -> &mut Self {
        self.port = port;
        self
    }

    pub fn static_service(&mut self, open: bool) -> &mut Self {
        self.static_service = open;
        self
    }

    pub fn with_shell(&mut self, w: SapperAppShellType) -> &mut Self {
        self.shell = Some(Arc::new(w));
        self
    }

    pub fn init_global(&mut self, clos: GlobalInitClosure) -> &mut Self {
        self.init_closure = Some(Arc::new(clos));
        self
    }

    // add methods of this sapper module
    pub fn add_module(&mut self, sm: Box<SapperModule>) -> &mut Self {
        let mut router = SapperRouter::new();
        // get the sm router
        sm.router(&mut router).unwrap();
        let sm = Arc::new(sm);

        for (method, handler_vec) in router.as_inner() {
            // add to wrapped router
            for &(glob, ref handler) in handler_vec.iter() {
                let method = method.clone();
                let glob = glob.clone();
                let handler = handler.clone();
                let sm = sm.clone();
                let shell = self.shell.clone();
                let init_closure = self.init_closure.clone();

                self.routers.route(method, glob, Arc::new(Box::new(move |req: &mut SapperRequest| -> Result<SapperResponse> {
                    if let Some(ref c) = init_closure {
                        c(req)?;
                    }
                    if let Some(ref shell) = shell {
                        shell.before(req)?;
                    }
                    sm.before(req)?;
                    let mut response: SapperResponse = handler.handle(req)?;
                    sm.after(req, &mut response)?;
                    if let Some(ref shell) = shell {
                        shell.after(req, &mut response)?;
                    }
                    Ok(response)
                })));
            }
        }

        self
    }

    pub fn run_http(self) {
        // thread_pool
        let num_cpus = num_cpus::get();
        let mut vec = Vec::with_capacity(num_cpus);
        for _ in 0..num_cpus {
            vec.push(worker());
        }
        let mut idx = 0;

        let addr = (self.address.clone() + ":" + &self.port.to_string()).parse().unwrap();
        //let self_box = Arc::new(Box::new(self));

        let mut core = Core::new().unwrap();
        let handle = core.handle();

        let new_service = const_service(self);

        let srv = Http::new().serve_addr_handle(&addr, &handle, new_service).unwrap();

        core.run(srv.for_each(|conn| {
            vec[idx % num_cpus].send(conn).expect("");
            idx += 1;
            Ok(())
        }).map_err(|err| println!("srv1 error: {:?}", err))).unwrap();

        //        let handle1 = handle.clone();
        //        handle.spawn(srv.for_each(move |conn| {
        //            handle1.spawn(conn.map(|_| ()).map_err(|err| println!("srv1 error: {:?}", err)));
        //            Ok(())
        //        }).map_err(|_| ()));
        //
        //        core.run(empty::<(), ()>()).unwrap();

        //        Server::http(&addr[..]).unwrap()
        //            .handle(self).unwrap();
    }
}

impl Service for SapperApp {
    type Request = Request;
    type Response = Response;
    type Error = HyperError;
    type Future = FutureResult<Self::Response, Self::Error>;

    fn call(&self, req: Self::Request) -> Self::Future {
        let mut sreq = SapperRequest::new(req);

        // pass req to routers, execute matched biz handler
        let response_w = self.routers.handle_method(&mut sreq).unwrap();

        let (path, query) = sreq.uri();

        if response_w.is_err() {
            let mut res = Response::new();
            if self.static_service {
                match simple_file_get(&path) {
                    Ok((file_u8vec, file_mime)) => {
                        res.headers_mut().set_raw("Content-Type", vec![file_mime.as_bytes().to_vec()]);
                        res.set_body(file_u8vec);
                    }
                    Err(_) => {
                        res.set_status(StatusCode::NotFound);
                        res.set_body("404 Not Found");
                    }
                }
            }
            return ok(res);
        } else {
            return ok(response_w.unwrap().into());
        }
    }
}




// this is very expensive in time
// should make it as global 
lazy_static! {
    static ref MTYPES: MimeTypes = { MimeTypes::new().unwrap() };
}

fn simple_file_get(path: &str) -> Result<(Vec<u8>, String)> {
    let new_path;
    if &path[(path.len() - 1)..] == "/" {
        new_path = "static/".to_owned() + path + "index.html";
    } else {
        new_path = "static/".to_owned() + path;
    }
    //println!("file path: {}", new_path);
    match File::open(&new_path) {
        Ok(ref mut file) => {
            let mut s: Vec<u8> = vec![];
            file.read_to_end(&mut s).unwrap_or(0);

            let mt_str = MTYPES.mime_for_path(Path::new(&new_path));

            Ok((s, mt_str.to_owned()))
        }
        Err(_) => Err(Error::FileNotExist)
    }
}


