use cosmwasm_std::testing::{mock_dependencies, mock_env, mock_info};
use cosmwasm_std::{Addr, DepsMut, Env, MessageInfo};

use crate::contract::instantiate;
use crate::msg::InstantiateMsg;
use crate::state::{Config, CONFIG};

fn _do_instantiate(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    factory: Addr,
    staking: Addr,
    astro_toke: Addr,
) {
    let instantiate_msg = InstantiateMsg {
        factory_contract: factory.to_string(),
        staking_contract: staking.to_string(),
        astro_token_contract: astro_toke.to_string(),
    };
    let res = instantiate(deps, _env.clone(), info.clone(), instantiate_msg).unwrap();
    assert_eq!(0, res.messages.len());
}

#[test]
fn proper_initialization() {
    let mut deps = mock_dependencies(&[]);
    let info = mock_info("addr0000", &[]);

    let env = mock_env();
    let factory = Addr::unchecked("factory");
    let staking = Addr::unchecked("staking");
    let astro_token_contract = Addr::unchecked("astro-token");

    let instantiate_msg = InstantiateMsg {
        factory_contract: factory.to_string(),
        staking_contract: staking.to_string(),
        astro_token_contract: astro_token_contract.to_string(),
    };
    let res = instantiate(deps.as_mut(), env, info, instantiate_msg).unwrap();
    assert_eq!(0, res.messages.len());

    let state = CONFIG.load(deps.as_mut().storage).unwrap();
    assert_eq!(
        state,
        Config {
            owner: Addr::unchecked("addr0000"),
            factory_contract: Addr::unchecked("factory"),
            staking_contract: Addr::unchecked("staking"),
            astro_token_contract: Addr::unchecked("astro-token"),
        }
    )
}

// should convert USDC - ASTRO
#[test]
fn convert_usdc_astro() {
    // let mut deps = mock_dependencies(&[]);
    // let info = mock_info("addr0000", &[]);
    //
    // let env = mock_env();
    // let factory = Addr::unchecked("factory");
    // let staking = Addr::unchecked("staking");
    // let astro_token_contract = Addr::unchecked("astro");
    // let astro_token = AssetInfo::Token {
    //     contract_addr: astro_token_contract.clone(),
    // };
    // let usdc_token = AssetInfo::Token {
    //     contract_addr: Addr::unchecked("usdc"),
    // };
    // let lp_token_contract = Addr::unchecked("usdc_astro");
    // let lp_token = AssetInfo::Token {
    //     contract_addr: lp_token_contract.clone(),
    // };
    //
    // _do_instantiate(
    //     deps.as_mut(),
    //     env.clone(),
    //     info.clone(),
    //     factory.clone(),
    //     staking.clone(),
    //     astro_token_contract.clone(),
    // );
    // deps.querier.set_balance(
    //     Addr::unchecked(MOCK_CONTRACT_ADDR),
    //     lp_token_contract.clone(),
    //     Uint128::from(1u128),
    // );

    // let res = execute(deps.as_mut(), env, info, ExecuteMsg::Convert { token1: usdc_token, token2: astro_token }).unwrap();
    //
    // assert_eq!(
    //     res.messages,
    //     vec![]
    // )

    // expect astro balanceOf Maker address equal 0
    // expect USDC_ASTRO balanceOf Maker address equal 0
    // expect astro balanceOf staking address equal "100"
}

// converts astro/USDC
#[test]
fn converts_astro_usdc() {
    // transfer ASTRO_USDC to Maker address 1
    // Maker convert(ASTRO address, USDC address)
    // expect astro balanceOf Maker address equal 0
    // expect ASTRO_USDC balanceOf Maker address equal 0
    // expect astro balanceOf staking address equal "100"
}

// converts DAI/USDC
#[test]
fn converts_dai_usdc() {
    // transfer DAI_USDC to Maker address 1
    // Maker convert(DAI address, USDC address)
    // expect astro balanceOf Maker address equal 0
    // expect DAI_USDC balanceOf Maker address equal 0
    // expect astro balanceOf staking address equal "100"
}

// reverts if pair does not exist
#[test]
fn pair_not_exist() {
    // expect Maker convert( USDC address, ASRTO_USDC address revertedWith ("Invalid pair")
}

// reverts if no path is available
#[test]
fn no_path_available() {
    // transfer ASTRO_USDC to Maker address 1
    // Maker.convert(this.mic.address, this.usdc.address)).to.be.revertedWith("SushiMaker: Cannot convert")
    // expect(await this.sushi.balanceOf(this.sushiMaker.address)).to.equal(0)
    // expect(await this.micUSDC.balanceOf(this.sushiMaker.address)).to.equal(getBigNumber(1))
    // expect(await this.sushi.balanceOf(this.bar.address)).to.equal(0)
}

// should allow to convert multiple
#[test]
fn convert_multiple() {
    // transfer LUNA_USDC to Maker address 1
    // transfer ASTRO_DAI to Maker address 1
    // Maker convertMultiple([LUNA address, ASTRO address], [USDC address, DAI.address])
    // expect astro balanceOf Maker address equal 0
    // expect LUNA_USDC balanceOf Maker address equal 0
    // expect ASTRO_DAI balanceOf Maker address equal 0
    // expect astro balanceOf staking address equal "100500"
}
