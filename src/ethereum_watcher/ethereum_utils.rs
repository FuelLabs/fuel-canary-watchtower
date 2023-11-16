use ethers::providers::{Provider, Http, Middleware};
use ethers::prelude::k256::ecdsa::SigningKey;
use std::sync::Arc;
use anyhow::{Result, anyhow};
use std::convert::TryFrom;
use std::ops::Mul;
use ethers::abi::AbiEncode;
use fuels::tx::Bytes32;
use ethers::prelude::{GasEscalatorMiddleware, Signer, Wallet, Log};
use ethers::middleware::gas_escalator::{Frequency, GeometricGasPrice};
use ethers::types::U256;

pub async fn setup_ethereum_provider(
    ethereum_rpc: &str,
) -> Result<Arc<GasEscalatorMiddleware<Provider<Http>>>> {
    // Geometrically increase gas price:
    // Start with `initial_price`, then increase it every 'every_secs' seconds by a fixed
    // coefficient. Coefficient defaults to 1.125 (12.5%), the minimum increase for Parity to
    // replace a transaction. Coefficient can be adjusted, and there is an optional upper limit.
    let coefficient: f64 = 1.125;
    let every_secs: u64 = 60;
    let max_price: Option<i32> = None;

    let geometric_escalator = GeometricGasPrice::new(
        coefficient,
        every_secs,
        max_price,
    );

    let provider = Provider::<Http>::try_from(ethereum_rpc)?;
    let provider = GasEscalatorMiddleware::new(
        provider,
        geometric_escalator,
        Frequency::PerBlock,
    );

    let provider_result = provider.get_chainid().await;
    match provider_result {
        Ok(_) => Ok(Arc::new(provider)),
        Err(e) => Err(anyhow!("Failed to get chain ID: {e}")),
    }
}

pub fn setup_ethereum_wallet(
    ethereum_wallet_key: Option<String>,
    chain_id: u64,
) -> Result<(Wallet<SigningKey>, bool)> {

    let mut read_only: bool = false;
    let key_str = ethereum_wallet_key.unwrap_or_else(|| {
        read_only = true;
        "ac0974bec39a17e36ba4a6b4d238ff944bacb478cbed5efcae784d7bf4f2ff80".to_string()
    });

    let wallet = key_str.parse::<Wallet<SigningKey>>()
        .map_err(|e| anyhow!("Failed to parse wallet key: {e}"))?
        .with_chain_id(chain_id);

    Ok((wallet, read_only))
}

pub fn get_public_address(key_str: &str) -> Result<String> {
    let wallet: Wallet<SigningKey> = key_str.parse::<Wallet<SigningKey>>()?;
    Ok(wallet.address().encode_hex())
}

pub fn get_value(value_fp: f64, decimals: u8) -> U256 {
    let decimals_p1 = if decimals < 9 { decimals } else { decimals - 9 };
    let decimals_p2 = decimals - decimals_p1;

    let value = value_fp * 10.0_f64.powf(decimals_p1 as f64);
    let value = U256::from(value as u64);

    value.mul(10_u64.pow(decimals_p2 as u32))
}

pub fn process_logs(logs: Vec<Log>) -> Result<Vec<Bytes32>> {
    let mut extracted_data = Vec::new();
    for log in logs {
        let mut bytes32_data: [u8; 32] = [0; 32];
        if log.data.len() == 32 {
            bytes32_data.copy_from_slice(&log.data);
            extracted_data.push(Bytes32::new(bytes32_data));
        } else {
            return Err(anyhow!("Length of log.data does not match that of 32"));
        }
    }
    Ok(extracted_data)
}