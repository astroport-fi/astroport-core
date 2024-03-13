use astroport::querier::query_factory_config;
use cosmwasm_std::{
    coin, wasm_execute, Addr, CosmosMsg, CustomMsg, CustomQuery, QuerierWrapper, Uint128,
};

use astroport::incentives::ExecuteMsg as IncentiveExecuteMsg;
use astroport::token_factory::tf_mint_msg;

use crate::error::ContractError;
use crate::state::Config;

/// Mint LP tokens for a beneficiary and auto stake the tokens in the Incentive contract (if auto staking is specified).
///
/// * **recipient** LP token recipient.
///
/// * **coin** denom and amount of LP tokens that will be minted for the recipient.
///
/// * **auto_stake** determines whether the newly minted LP tokens will
/// be automatically staked in the Generator on behalf of the recipient.
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
    dbg!(&coin);

    // If no auto-stake - just mint to recipient
    if !auto_stake {
        return Ok(vec![tf_mint_msg(contract_address, coin, recipient)]);
    }

    // Mint for the pair contract and stake into the Generator contract
    let generator = query_factory_config(&querier, &config.factory_addr)?.generator_address;

    if let Some(generator) = generator {
        Ok(vec![
            tf_mint_msg(contract_address, coin.clone(), recipient),
            wasm_execute(
                generator,
                &IncentiveExecuteMsg::Deposit {
                    recipient: Some(recipient.to_string()),
                },
                vec![coin],
            )?
            .into(),
        ])
    } else {
        Err(ContractError::AutoStakeError {})
    }
}
