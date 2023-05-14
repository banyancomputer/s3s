use super::Body;

use hyper::http::HeaderValue;
use hyper::HeaderMap;
use hyper::StatusCode;

#[derive(Default)]
pub struct Response {
    pub status: StatusCode,
    pub headers: HeaderMap<HeaderValue>,
    pub body: Body,
}

impl From<Response> for hyper::Response<Body> {
    fn from(res: Response) -> Self {
        let mut ans = hyper::Response::default();
        *ans.status_mut() = res.status;
        *ans.headers_mut() = res.headers;
        *ans.body_mut() = res.body;
        ans
    }
}

impl Response {
    #[must_use]
    pub fn with_status(status: StatusCode) -> Self {
        Self {
            status,
            ..Default::default()
        }
    }
}

impl From<Response> for worker::Response {
    fn from(res: Response) -> Self {
        todo!("Response -> worker::Response")
    }
}
