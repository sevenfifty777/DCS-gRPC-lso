use stubs::metadata;
use stubs::metadata::v0::metadata_service_client::MetadataServiceClient;
use tonic::transport::Channel;

use super::{request_with_deadline, GrpcResult};

pub struct MetadataClient {
    svc: MetadataServiceClient<Channel>,
}

impl MetadataClient {
    pub fn new(channel: Channel) -> Self {
        Self {
            svc: MetadataServiceClient::new(channel),
        }
    }

    pub async fn get_version(&mut self) -> GrpcResult<String> {
        Ok(self
            .svc
            .get_version(request_with_deadline(metadata::v0::GetVersionRequest {}))
            .await
            .map_err(Box::new)?
            .into_inner()
            .version)
    }
}
