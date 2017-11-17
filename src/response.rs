use hyper::{StatusCode, Headers, Response};

pub struct SapperResponse {
    status: StatusCode,
    headers: Headers,
    body: Option<Vec<u8>>,
}


impl SapperResponse {
    pub fn new() -> SapperResponse {
        SapperResponse {
            status: StatusCode::Ok,
            headers: Headers::new(),
            body: None
        }
    }

    pub fn status(&self) -> StatusCode {
        self.status
    }

    pub fn set_status(&mut self, status: StatusCode) {
        self.status = status;
    }


    pub fn headers(&self) -> &Headers {
        &self.headers
    }

    pub fn headers_mut(&mut self) -> &mut Headers {
        &mut self.headers
    }


    pub fn body(&self) -> &Option<Vec<u8>> {
        &self.body
    }

    pub fn write_body(&mut self, body: String) {
        self.body = Some(body.as_bytes().to_vec())
    }

    pub fn write_raw_body(&mut self, body: Vec<u8>) {
        self.body = Some(body)
    }
}

impl Into<Response> for SapperResponse {
    fn into(self) -> Response {
        let mut response = Response::new()
            .with_status(self.status)
            .with_headers(self.headers);
        self.body.map(|vec| {
            response.set_body(vec)
        });
        response
    }
}
