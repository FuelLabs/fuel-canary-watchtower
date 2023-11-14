use std::fmt::Error;
use super::{FUEL_BLOCK_TIME};

use fuels::prelude::{abigen, Provider, Contract, WalletUnlocked, Bech32ContractId};

use std::sync::Arc;
use anyhow::Result;

abigen!(
    Contract(
        name="BridgeFungibleTokenContract",
        abi="./abi/BridgeFungibleTokenContract.json"
    ),
);

pub struct FungibleTokenContract {
    provider: Arc<Provider>,
    contract: Option<BridgeFungibleTokenContract<WalletUnlocked>>,
    address: Bech32ContractId,
}

impl FungibleTokenContract {

    pub fn new(provider: Arc<Provider>, address: Bech32ContractId) -> Result<Self> {
        Ok(FungibleTokenContract {provider, contract: None, address})
    }

    pub async fn initialize(
        &mut self,
        wallet: &mut WalletUnlocked,
    ) -> Result<(), Error> {

        let token_contract = BridgeFungibleTokenContract::new(
            self.address.clone(),
            wallet.clone(),
        );

        self.contract = Some(token_contract);

        Ok(())
    }

    pub async fn get_amount_withdrawn(&self, timeframe: u32, _token_address: &str) -> Result<u64> {
        let _block_offset = timeframe as u64 / FUEL_BLOCK_TIME;
        // TODO

        Ok(0)
    }
}
