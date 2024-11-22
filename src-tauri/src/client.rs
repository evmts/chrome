use helios::ethereum::{
    config::networks::Network, database::FileDB, EthereumClient, EthereumClientBuilder,
};
use std::path::PathBuf;

pub struct LightClient {
    client: EthereumClient<FileDB>
}

impl LightClient {

    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let untrusted_rpc_url = "https://eth-mainnet.g.alchemy.com/v2/";
        let consensus_rpc = "https://www.lightclientdata.org";

        let client = EthereumClientBuilder::new()
            .network(Network::MAINNET)
            .consensus_rpc(consensus_rpc)
            .execution_rpc(untrusted_rpc_url)
            .load_external_fallback()
            .data_dir(PathBuf::from("/tmp/helios"))
            .build()?;
        
        Ok(LightClient{client})
    }

    pub async fn start(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.client.start().await?;
        Ok(())
    }

    pub async fn wait_synced(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        self.client.wait_synced().await;
        Ok(())
    }
}
