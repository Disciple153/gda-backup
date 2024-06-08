use aws_config::meta::region::RegionProviderChain;
use aws_config::BehaviorVersion;

/// The function `get_config` asynchronously retrieves the AWS SDK configuration
/// with a default region provider set to "us-east-1".
pub async fn get_config() -> aws_config::SdkConfig {
    let region_provider = RegionProviderChain::default_provider().or_else("us-east-1");
    aws_config::defaults(BehaviorVersion::latest())
        .region(region_provider)
        .load()
        .await
}