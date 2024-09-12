use prost::Message;

use crate::error::LdkNodeServerError;
use protos::{
	Bolt11ReceiveRequest, Bolt11ReceiveResponse, Bolt11SendRequest, Bolt11SendResponse,
	Bolt12ReceiveRequest, Bolt12ReceiveResponse, Bolt12SendRequest, Bolt12SendResponse,
	CloseChannelRequest, CloseChannelResponse, ListChannelsRequest, ListChannelsResponse,
	OnchainReceiveRequest, OnchainReceiveResponse, OnchainSendRequest, OnchainSendResponse,
	OpenChannelRequest, OpenChannelResponse,
};
use reqwest::header::CONTENT_TYPE;
use reqwest::Client;

const APPLICATION_OCTET_STREAM: &str = "application/octet-stream";

const ONCHAIN_RECEIVE_PATH: &str = "OnchainReceive";
const ONCHAIN_SEND_PATH: &str = "OnchainSend";
const BOLT11_RECEIVE_PATH: &str = "Bolt11Receive";
const BOLT11_SEND_PATH: &str = "Bolt11Send";
const BOLT12_RECEIVE_PATH: &str = "Bolt12Receive";
const BOLT12_SEND_PATH: &str = "Bolt12Send";
const OPEN_CHANNEL_PATH: &str = "OpenChannel";
const CLOSE_CHANNEL_PATH: &str = "CloseChannel";
const LIST_CHANNELS_PATH: &str = "ListChannels";

/// Client to access a hosted instance of LDK Node Server.
#[derive(Clone)]
pub struct LdkNodeServerClient {
	base_url: String,
	client: Client,
}

impl LdkNodeServerClient {
	/// Constructs a [`LdkNodeServerClient`] using `base_url` as the server endpoint.
	pub fn new(base_url: String) -> Self {
		Self { base_url, client: Client::new() }
	}

	/// Retrieve a new on-chain funding address.
	/// For API contract/usage, refer to docs for [`OnchainReceiveRequest`] and [`OnchainReceiveResponse`].
	pub async fn onchain_receive(
		&self, request: OnchainReceiveRequest,
	) -> Result<OnchainReceiveResponse, LdkNodeServerError> {
		let url = format!("http://{}/{ONCHAIN_RECEIVE_PATH}", self.base_url);
		self.post_request(&request, &url).await
	}

	/// Send an on-chain payment to the given address.
	/// For API contract/usage, refer to docs for [`OnchainSendRequest`] and [`OnchainSendResponse`].
	pub async fn onchain_send(
		&self, request: OnchainSendRequest,
	) -> Result<OnchainSendResponse, LdkNodeServerError> {
		let url = format!("http://{}/{ONCHAIN_SEND_PATH}", self.base_url);
		self.post_request(&request, &url).await
	}

	/// Retrieve a new BOLT11 payable invoice.
	/// For API contract/usage, refer to docs for [`Bolt11ReceiveRequest`] and [`Bolt11ReceiveResponse`].
	pub async fn bolt11_receive(
		&self, request: Bolt11ReceiveRequest,
	) -> Result<Bolt11ReceiveResponse, LdkNodeServerError> {
		let url = format!("http://{}/{BOLT11_RECEIVE_PATH}", self.base_url);
		self.post_request(&request, &url).await
	}

	/// Send a payment for a BOLT11 invoice.
	/// For API contract/usage, refer to docs for [`Bolt11SendRequest`] and [`Bolt11SendResponse`].
	pub async fn bolt11_send(
		&self, request: Bolt11SendRequest,
	) -> Result<Bolt11SendResponse, LdkNodeServerError> {
		let url = format!("http://{}/{BOLT11_SEND_PATH}", self.base_url);
		self.post_request(&request, &url).await
	}

	/// Retrieve a new BOLT11 payable offer.
	/// For API contract/usage, refer to docs for [`Bolt12ReceiveRequest`] and [`Bolt12ReceiveResponse`].
	pub async fn bolt12_receive(
		&self, request: Bolt12ReceiveRequest,
	) -> Result<Bolt12ReceiveResponse, LdkNodeServerError> {
		let url = format!("http://{}/{BOLT12_RECEIVE_PATH}", self.base_url);
		self.post_request(&request, &url).await
	}

	/// Send a payment for a BOLT12 offer.
	/// For API contract/usage, refer to docs for [`Bolt12SendRequest`] and [`Bolt12SendResponse`].
	pub async fn bolt12_send(
		&self, request: Bolt12SendRequest,
	) -> Result<Bolt12SendResponse, LdkNodeServerError> {
		let url = format!("http://{}/{BOLT12_SEND_PATH}", self.base_url);
		self.post_request(&request, &url).await
	}

	/// Creates a new outbound channel.
	/// For API contract/usage, refer to docs for [`OpenChannelRequest`] and [`OpenChannelResponse`].
	pub async fn open_channel(
		&self, request: OpenChannelRequest,
	) -> Result<OpenChannelResponse, LdkNodeServerError> {
		let url = format!("http://{}/{OPEN_CHANNEL_PATH}", self.base_url);
		self.post_request(&request, &url).await
	}

	/// Closes the channel specified by given request.
	/// For API contract/usage, refer to docs for [`CloseChannelRequest`] and [`CloseChannelResponse`].
	pub async fn close_channel(
		&self, request: CloseChannelRequest,
	) -> Result<CloseChannelResponse, LdkNodeServerError> {
		let url = format!("http://{}/{CLOSE_CHANNEL_PATH}", self.base_url);
		self.post_request(&request, &url).await
	}

	/// Retrieves list of known channels.
	/// For API contract/usage, refer to docs for [`ListChannelsRequest`] and [`ListChannelsResponse`].
	pub async fn list_channels(
		&self, request: ListChannelsRequest,
	) -> Result<ListChannelsResponse, LdkNodeServerError> {
		let url = format!("http://{}/{LIST_CHANNELS_PATH}", self.base_url);
		self.post_request(&request, &url).await
	}

	async fn post_request<Rq: Message, Rs: Message + Default>(
		&self, request: &Rq, url: &str,
	) -> Result<Rs, LdkNodeServerError> {
		let request_body = request.encode_to_vec();
		let response_raw = match self
			.client
			.post(url)
			.header(CONTENT_TYPE, APPLICATION_OCTET_STREAM)
			.body(request_body)
			.send()
			.await
		{
			Ok(response) => response,
			Err(e) => {
				return Err(LdkNodeServerError::InternalError(e.to_string()));
			},
		};
		let status = response_raw.status();
		let payload = response_raw.bytes().await?;

		if status.is_success() {
			Ok(Rs::decode(&payload[..])?)
		} else {
			//TODO: Error handling and error response parsing.
			Err(LdkNodeServerError::InternalError("Unknown Error".to_string()))
		}
	}
}