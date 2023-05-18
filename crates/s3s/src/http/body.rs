use crate::error::StdError;
use crate::stream::ByteStream;
use crate::stream::RemainingLength;

use std::fmt;
use std::mem;
use std::pin::Pin;
use std::task::Context;
use std::task::Poll;

use bytes::Bytes;
use futures::Stream;

pin_project_lite::pin_project! {
    #[derive(Default)]
    pub struct Body {
        #[pin]
        kind: Kind,
    }
}

pin_project_lite::pin_project! {
    #[project = KindProj]
    #[derive(Default)]
    enum Kind {
        #[default]
        Empty,
        Once {
            inner: Bytes,
        },
        ByteStream {
            #[pin]
            inner: Pin<Box<worker::ByteStream>>,
        }
    }
}

impl Body {
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    fn once(bytes: Bytes) -> Self {
        Self {
            kind: Kind::Once { inner: bytes },
        }
    }

    fn stream(stream: worker::ByteStream) -> Self {
        Self {
            kind: Kind::ByteStream { inner: Box::pin(stream) },
        }
    }
}

impl From<Bytes> for Body {
    fn from(bytes: Bytes) -> Self {
        Self::once(bytes)
    }
}

impl From<Vec<u8>> for Body {
    fn from(value: Vec<u8>) -> Self {
        Self::once(value.into())
    }
}

impl From<String> for Body {
    fn from(value: String) -> Self {
        Self::once(value.into())
    }
}

// mess...
// impl<s: From<impl Stream<Item=Bytes>> for Body {
//     fn from(stream: impl Stream<Item=Bytes>) -> Self {
//         Self::stream(stream)
//     }
// }

impl http_body::Body for Body {
    type Data = Bytes;

    type Error = worker::Error;

    fn poll_data(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<Self::Data, Self::Error>>> {
        let mut this = self.project();
        match this.kind.as_mut().project() {
            KindProj::Empty => {
                Poll::Ready(None) //
            }
            KindProj::Once { inner } => {
                let bytes = mem::take(inner);
                this.kind.set(Kind::Empty);
                if bytes.is_empty() {
                    Poll::Ready(None)
                } else {
                    Poll::Ready(Some(Ok(bytes)))
                }
            }
            KindProj::ByteStream { mut inner } => {
                let polled = {inner.as_mut().poll_next(cx)};
                match polled {
                    Poll::Ready(Some(Ok(value))) => {
                        let bytes = Bytes::from(value);
                        Poll::Ready(Some(Ok(bytes)))
                    }
                    Poll::Ready(Some(Err(err))) => {
                        Poll::Ready(Some(Err(err)))
                    }
                    Poll::Ready(None) => {
                        Poll::Ready(None)
                    }
                    Poll::Pending => {
                        Poll::Pending
                    }
                }
            }     
        }
    }

    fn poll_trailers(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Result<Option<hyper::HeaderMap>, Self::Error>> {
        Poll::Ready(Ok(None)) // TODO: How to impl poll_trailers?
    }

    fn is_end_stream(&self) -> bool {
        match &self.kind {
            Kind::Empty => true,
            Kind::Once { inner } => inner.is_empty(),
            // TODO is this correct
            Kind::ByteStream { inner } => inner.size_hint().1.map_or(false, |v| v == 0),
        }
    }

    fn size_hint(&self) -> http_body::SizeHint {
        match &self.kind {
            Kind::Empty => http_body::SizeHint::with_exact(0),
            Kind::Once { inner } => http_body::SizeHint::with_exact(inner.len() as u64),
            Kind::ByteStream { inner } => {
                let (lower,upper) = inner.size_hint();
                http_body::SizeHint {
                    upper: upper.map(|v| v as u64),
                    lower: lower as u64,
                }
            }
        }
    }
}

impl Stream for Body {
    type Item = Result<Bytes, worker::Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        http_body::Body::poll_data(self, cx)
    }
}

impl ByteStream for Body {
    fn remaining_length(&self) -> RemainingLength {
        RemainingLength::from(http_body::Body::size_hint(self))
    }
}

impl fmt::Debug for Body {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut d = f.debug_struct("Body");
        match &self.kind {
            Kind::Empty => {}
            Kind::Once { inner } => {
                d.field("once", inner);
            }
            Kind::ByteStream { inner } => {
                d.field("dyn_stream", &"{..}");
                d.field("remaining_length", &self.remaining_length());
            }
        }
        d.finish()
    }
}

impl Body {
    /// Stores all bytes in memory.
    ///
    /// WARNING: This function may cause **unbounded memory allocation**.
    ///
    /// # Errors
    /// Returns an error if `hyper` fails to read the body.
    pub async fn store_all_unlimited(&mut self) -> Result<Bytes, StdError> {
        if let Some(bytes) = self.bytes() {
            return Ok(bytes);
        }
        let body = mem::take(self);
        let bytes = hyper::body::to_bytes(body).await?;
        *self = Self::from(bytes.clone());
        Ok(bytes)
    }

    pub fn bytes(&self) -> Option<Bytes> {
        match &self.kind {
            Kind::Empty => Some(Bytes::new()),
            Kind::Once { inner } => Some(inner.clone()),
            _ => None,
        }
    }

    // fn into_writable_stream(&self) -> web_sys::WritableStream {
    //     match self.kind {
    //         Kind::Empty => Stream<Item=Bytes>::empty().into(),
    //         Kind::Once { inner } => inner.into(),
    //         Kind::ReadableStream { inner } => todo!("ReadableStream -> WritableStream"),
    //     }
    // }
}

// impl From<Body> for WritableStream {
//     fn from(body: Body) -> Self {
//         body.into_writable_stream()
//     }
// }