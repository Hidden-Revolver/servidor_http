use std::collections::HashMap;

use crate::package;
use crate::router::Route;

pub use crate::package::Package;

mod cookie_list;
mod method;
mod query;

pub use cookie_list::CookieList;
pub use method::Method;
pub use query::Query;

/// Represents a request made by a client.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    /// The route of the request.
    pub path: Route,

    /// The query of the request.
    pub query: Option<Query>,

    /// The cookies of the request.
    pub cookies: CookieList,

    headers: HashMap<String, String>,
    body: Option<Vec<u8>>,
}

package::generate_package_getters_setters!(Request[Vec<u8>]);

impl Request {
    /// Generates a new request method, with the given method and path.
    pub fn new(method: Method, path: &str, query: Option<Query>) -> Self {
        let path = Route::new(method, path);

        Request {
            path,
            headers: HashMap::new(),
            query,
            cookies: CookieList::new(),
            body: None,
        }
    }

    /// Returns the body of the request as a string.
    pub fn get_body_string(&self) -> String {
        match &self.body {
            Some(body) => String::from_utf8_lossy(body).to_string(),
            None => String::new(),
        }
    }

    fn parse_header_str(header_string: &str) -> Result<Request, crate::Error> {
        let mut lines = header_string.lines();

        let mut request = match lines.next() {
            Some(request_line) => {
                let mut request_line_parts = request_line.split_whitespace();

                let request_method_string = match request_line_parts.next() {
                    Some(method) => method,
                    None => {
                        return Err(crate::Error::RequestError(RequestError::InvalidRequest(
                            String::from(header_string),
                        )))
                    }
                };

                let request_method = match Method::try_from(request_method_string) {
                    Ok(method) => method,
                    Err(_) => {
                        return Err(crate::Error::RequestError(
                            RequestError::InvalidRequestMethod(String::from(request_method_string)),
                        ))
                    }
                };

                let request_path_with_query = match request_line_parts.next() {
                    Some(url) => url,
                    None => return Err(crate::Error::RequestError(RequestError::NoUrlFound)),
                };

                let (request_path, query) = match request_path_with_query.contains('?') {
                    true => {
                        let mut url_and_query = request_path_with_query.splitn(2, '?');

                        let request_path = match url_and_query.next() {
                            Some(url) => url,
                            None => {
                                return Err(crate::Error::RequestError(RequestError::NoUrlFound))
                            }
                        };

                        let query_string = match url_and_query.next() {
                            Some(query) => query,
                            None => {
                                return Err(crate::Error::RequestError(RequestError::QueryError(
                                    request_path_with_query.to_string(),
                                )))
                            }
                        };

                        let query = Query::try_from(query_string)?;

                        (request_path, Some(query))
                    }
                    false => (request_path_with_query, None),
                };

                let http_version = match request_line_parts.next() {
                    Some(version) => version,
                    None => {
                        return Err(crate::Error::RequestError(RequestError::InvalidRequest(
                            String::from(header_string),
                        )))
                    }
                };

                if !http_version.contains("HTTP/") {
                    return Err(crate::Error::RequestError(
                        RequestError::HttpVersionNotSupported(String::from(http_version)),
                    ));
                }

                Request::new(request_method, request_path, query)
            }
            None => {
                return Err(crate::Error::RequestError(RequestError::InvalidRequest(
                    String::from(header_string),
                )));
            }
        };

        for header in lines.by_ref() {
            if header.is_empty() {
                break;
            }

            let mut header_parts = header.splitn(2, ':');

            let header_key = match header_parts.next() {
                Some(key) => key,
                None => {
                    return Err(crate::Error::RequestError(RequestError::InvalidHeader(
                        String::from(header),
                    )))
                }
            };

            let header_value = match header_parts.next() {
                Some(value) => value.trim(),
                None => {
                    return Err(crate::Error::RequestError(RequestError::InvalidHeader(
                        String::from(header),
                    )))
                }
            };

            request.add_header(header_key, header_value);
        }

        let _a = request.get_header_list().get("Cookie");
        if let Some(cookies) = request.get_header_list().get("Cookie") {
            let cookie_list = CookieList::try_from(cookies.as_str())?;

            request.cookies = cookie_list;
        }

        Ok(request)
    }
}

impl From<Route> for Request {
    fn from(path: Route) -> Self {
        Request::new(path.method, path.path.as_str(), None)
    }
}

impl TryFrom<&str> for Request {
    type Error = crate::Error;

    fn try_from(req: &str) -> Result<Self, Self::Error> {
        let request = Request::parse_header_str(req)?;

        Ok(request)
    }
}

macro_rules! split_sequence {
    ($sequence:expr, $($separator:expr),*) => {{
        let mut header = Vec::new();
        let mut body = $sequence.to_vec();

        $(
            if let Some(pos) = body
                .windows($separator.len())
                .position(|window| window == $separator)
            {
                let (header_reference, body_reference) = body.split_at(pos);
                header = header_reference.to_vec();
                body = body_reference[$separator.len()..].to_vec();
            }
        )*

        (header, body)
    }};
}

impl TryFrom<Vec<u8>> for Request {
    type Error = crate::Error;

    fn try_from(binary_data: Vec<u8>) -> Result<Self, Self::Error> {
        let (header, body) = split_sequence!(binary_data, b"\r\n\r\n", b"\n\n");

        let header_string = String::from_utf8_lossy(&header);

        let mut request = Request::parse_header_str(header_string.as_ref())?;

        request.set_body(body);

        Ok(request)
    }
}

/// Contains all the possible errors that can occur when handling a request.
#[derive(Debug, thiserror::Error)]
pub enum RequestError {
    /// The request is invalid (Either is empty or lacks an http version).
    /// Caused by a user client
    #[error("Invalid request\nRaw data:\n{0}")]
    InvalidRequest(String),

    /// The request method is invalid. Check [crate::request::RequestMethod] for valid methods.
    #[error("Invalid request method: {0}")]
    InvalidRequestMethod(String),

    /// No URL was found in the request.
    #[error("No URL found in request")]
    NoUrlFound,

    /// The HTTP version is not supported.
    #[error("HTTP version not supported: {0}")]
    HttpVersionNotSupported(String),

    /// The header is invalid (Doesn't follow the `"key":"value"` scheme).
    #[error("Invalid header")]
    InvalidHeader(String),

    /// Error while getting the query from the request
    #[error("Error parsing query: {0}")]
    QueryError(String),

    /// Error while parsing cookies
    #[error("Error parsing cookies: {0}")]
    CookieError(String),
}
