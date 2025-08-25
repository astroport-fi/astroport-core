use crate::error::ContractError;
use crate::state::Config;
use astroport::asset::{Asset, PairInfo};
use astroport::incentives;
use astroport::querier::query_factory_config;
use astroport::token_factory::tf_mint_msg;
use cosmwasm_std::{
    coin, ensure, ensure_eq, wasm_execute, Addr, CosmosMsg, CustomMsg, CustomQuery, QuerierWrapper,
    StdError, Uint128,
};
use itertools::Itertools;

pub fn ensure_min_assets_to_receive(
    pair_info: &PairInfo,
    refund_assets: &[Asset],
    min_assets_to_receive: Option<Vec<Asset>>,
) -> Result<(), ContractError> {
    if let Some(mut min_assets_to_receive) = min_assets_to_receive {
        ensure_eq!(
            min_assets_to_receive.len(),
            refund_assets.len(),
            ContractError::WrongAssetLength {
                expected: refund_assets.len(),
                actual: min_assets_to_receive.len(),
            }
        );

        // Ensure unique
        ensure!(
            min_assets_to_receive
                .iter()
                .map(|asset| &asset.info)
                .all_unique(),
            StdError::generic_err("Duplicated assets in min_assets_to_receive")
        );

        for asset in &min_assets_to_receive {
            ensure!(
                pair_info.asset_infos.contains(&asset.info),
                ContractError::InvalidAsset(asset.info.to_string())
            );
        }

        if refund_assets[0].info.ne(&min_assets_to_receive[0].info) {
            min_assets_to_receive.swap(0, 1)
        }

        ensure!(
            refund_assets[0].amount >= min_assets_to_receive[0].amount,
            ContractError::WithdrawSlippageViolation {
                asset_name: refund_assets[0].info.to_string(),
                received: refund_assets[0].amount,
                expected: min_assets_to_receive[0].amount,
            }
        );

        ensure!(
            refund_assets[1].amount >= min_assets_to_receive[1].amount,
            ContractError::WithdrawSlippageViolation {
                asset_name: refund_assets[1].info.to_string(),
                received: refund_assets[1].amount,
                expected: min_assets_to_receive[1].amount,
            }
        );
    }

    Ok(())
}

/// Mint LP tokens for a beneficiary and auto stake the tokens in the Incentive contract (if auto staking is specified).
///
/// * **recipient** LP token recipient.
///
/// * **coin** denom and amount of LP tokens that will be minted for the recipient.
///
/// * **auto_stake** determines whether the newly minted LP tokens will
///   be automatically staked in the Incentives contract on behalf of the recipient.
pub fn mint_liquidity_token_message<T, C>(
    querier: QuerierWrapper<C>,
    config: &Config,
    contract_address: &Addr,
    recipient: &Addr,
    amount: Uint128,
    auto_stake: bool,
) -> Result<Vec<CosmosMsg<T>>, ContractError>
where
    C: CustomQuery,
    T: CustomMsg,
{
    let coin = coin(amount.into(), config.pair_info.liquidity_token.to_string());

    // If no auto-stake - just mint to recipient
    if !auto_stake {
        return Ok(tf_mint_msg(contract_address, coin, recipient));
    }

    // Mint for the pair contract and stake into the Incentives contract
    let incentives_addr = query_factory_config(&querier, &config.factory_addr)?.generator_address;

    if let Some(address) = incentives_addr {
        let mut msgs = tf_mint_msg(contract_address, coin.clone(), contract_address);
        msgs.push(
            wasm_execute(
                address,
                &incentives::ExecuteMsg::Deposit {
                    recipient: Some(recipient.to_string()),
                },
                vec![coin],
            )?
            .into(),
        );
        Ok(msgs)
    } else {
        Err(ContractError::IncentivesNotFound {})
    }
}
