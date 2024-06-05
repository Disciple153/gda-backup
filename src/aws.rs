use aws_config::meta::region::RegionProviderChain;
use aws_config::BehaviorVersion;

pub async fn get_config() -> aws_config::SdkConfig {
    let region_provider = RegionProviderChain::default_provider().or_else("us-east-1");
    aws_config::defaults(BehaviorVersion::latest())
        .region(region_provider)
        .load()
        .await
}