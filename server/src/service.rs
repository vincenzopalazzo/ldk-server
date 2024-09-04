use ldk_node::Node;

use http_body_util::{BodyExt, Full};
use hyper::body::{Bytes, Incoming};
use hyper::service::Service;
use hyper::{Request, Response, StatusCode};

use prost::Message;

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use crate::api::onchain_receive::handle_onchain_receive_request;
use crate::api::onchain_receive::ONCHAIN_RECEIVE_PATH;
use crate::api::onchain_send::handle_onchain_send_request;
use crate::api::onchain_send::ONCHAIN_SEND_PATH;
use crate::api::bolt11_receive::handle_bolt11_receive_request;
use crate::api::bolt11_receive::BOLT11_RECEIVE_PATH;

#[derive(Clone)]
pub struct NodeService {
	node: Arc<Node>,
}

impl NodeService {
	pub(crate) fn new(node: Arc<Node>) -> Self {
		Self { node }
	}
}

impl Service<Request<Incoming>> for NodeService {
	type Response = Response<Full<Bytes>>;
	type Error = hyper::Error;
	type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

	fn call(&self, req: Request<Incoming>) -> Self::Future {
		let node = Arc::clone(&self.node);
		match req.uri().path() {
			ONCHAIN_RECEIVE_PATH => {
				Box::pin(handle_request(node, req, handle_onchain_receive_request))
			},
			ONCHAIN_SEND_PATH => Box::pin(handle_request(node, req, handle_onchain_send_request)),
			BOLT11_RECEIVE_PATH => {
				Box::pin(handle_request(node, req, handle_bolt11_receive_request))
			},
			path => {
				let error = format!("Unknown request: {}", path).into_bytes();
				Box::pin(async {
					Ok(Response::builder()
						.status(StatusCode::BAD_REQUEST)
						.body(Full::new(Bytes::from(error)))
						// unwrap safety: body only errors when previous chained calls failed.
						.unwrap())
				})
			},
		}
	}
}

async fn handle_request<
	T: Message + Default,
	R: Message,
	F: Fn(Arc<Node>, T) -> Result<R, ldk_node::NodeError>,
>(
	node: Arc<Node>, request: Request<Incoming>, handler: F,
) -> Result<<NodeService as Service<Request<Incoming>>>::Response, hyper::Error> {
	// TODO: we should bound the amount of data we read to avoid allocating too much memory.
	let bytes = request.into_body().collect().await?.to_bytes();
	match T::decode(bytes) {
		Ok(request) => match handler(node, request) {
			Ok(response) => Ok(Response::builder()
				.body(Full::new(Bytes::from(response.encode_to_vec())))
				// unwrap safety: body only errors when previous chained calls failed.
				.unwrap()),
			Err(e) => Ok(Response::builder()
				.status(StatusCode::INTERNAL_SERVER_ERROR)
				.body(Full::new(Bytes::from(e.to_string().into_bytes())))
				// unwrap safety: body only errors when previous chained calls failed.
				.unwrap()),
		},
		Err(_) => Ok(Response::builder()
			.status(StatusCode::BAD_REQUEST)
			.body(Full::new(Bytes::from(b"Error parsing request".to_vec())))
			// unwrap safety: body only errors when previous chained calls failed.
			.unwrap()),
	}
}
