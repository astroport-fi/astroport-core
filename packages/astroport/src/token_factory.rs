pub use cosmos_sdk_proto::cosmos::base::v1beta1::Coin as ProtoCoin;
use cosmwasm_std::{Binary, Coin, CosmosMsg, CustomMsg, StdError};

#[cfg(any(feature = "injective", feature = "sei"))]
use cosmwasm_std::BankMsg;

use prost::Message;

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MsgCreateDenomResponse {
    #[prost(string, tag = "1")]
    pub new_token_denom: ::prost::alloc::string::String,
}

impl MsgCreateDenomResponse {
    pub fn to_proto_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        self.encode(&mut buf).unwrap();
        buf
    }
}

impl From<MsgCreateDenomResponse> for Binary {
    fn from(msg: MsgCreateDenomResponse) -> Self {
        Binary(msg.to_proto_bytes())
    }
}

impl TryFrom<Binary> for MsgCreateDenomResponse {
    type Error = StdError;
    fn try_from(binary: Binary) -> Result<Self, Self::Error> {
        Self::decode(binary.as_slice()).map_err(|e| {
            StdError::generic_err(
                format!(
                    "MsgCreateDenomResponse Unable to decode binary: \n  - base64: {}\n  - bytes array: {:?}\n\n{:?}",
                    binary,
                    binary.to_vec(),
                    e
                ),
            )
        })
    }
}

#[cfg(not(any(feature = "sei")))]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MsgCreateDenom {
    #[prost(string, tag = "1")]
    pub sender: ::prost::alloc::string::String,
    /// subdenom can be up to 44 "alphanumeric" characters long.
    #[prost(string, tag = "2")]
    pub subdenom: ::prost::alloc::string::String,
}

#[cfg(feature = "sei")]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MsgCreateDenom {
    /// subdenom can be up to 44 "alphanumeric" characters long.
    #[prost(string, tag = "2")]
    pub subdenom: ::prost::alloc::string::String,
}

impl MsgCreateDenom {
    #[cfg(not(any(feature = "injective", feature = "sei")))]
    pub const TYPE_URL: &'static str = "/osmosis.tokenfactory.v1beta1.MsgCreateDenom";
    #[cfg(feature = "injective")]
    pub const TYPE_URL: &'static str = "/injective.tokenfactory.v1beta1.MsgCreateDenom";
    #[cfg(feature = "sei")]
    pub const TYPE_URL: &'static str = "/seiprotocol.seichain.tokenfactory.v1beta1.MsgCreateDenom";
}

impl TryFrom<Binary> for MsgCreateDenom {
    type Error = StdError;
    fn try_from(binary: Binary) -> Result<Self, Self::Error> {
        Self::decode(binary.as_slice()).map_err(|e| {
            StdError::generic_err(format!(
                "MsgCreateDenom Unable to decode binary: \n  - base64: {}\n  - bytes array: {:?}\n\n{:?}",
                binary,
                binary.to_vec(),
                e
            ))
        })
    }
}

#[cfg(not(any(feature = "injective", feature = "sei")))]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MsgBurn {
    #[prost(string, tag = "1")]
    pub sender: ::prost::alloc::string::String,
    #[prost(message, optional, tag = "2")]
    pub amount: ::core::option::Option<cosmos_sdk_proto::cosmos::base::v1beta1::Coin>,
    #[prost(string, tag = "3")]
    pub burn_from_address: ::prost::alloc::string::String,
}

#[cfg(feature = "injective")]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MsgBurn {
    #[prost(string, tag = "1")]
    pub sender: ::prost::alloc::string::String,
    #[prost(message, optional, tag = "2")]
    pub amount: ::core::option::Option<cosmos_sdk_proto::cosmos::base::v1beta1::Coin>,
}

#[cfg(feature = "sei")]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MsgBurn {
    #[prost(message, optional, tag = "2")]
    pub amount: ::core::option::Option<cosmos_sdk_proto::cosmos::base::v1beta1::Coin>,
}

impl MsgBurn {
    #[cfg(not(any(feature = "injective", feature = "sei")))]
    pub const TYPE_URL: &'static str = "/osmosis.tokenfactory.v1beta1.MsgBurn";
    #[cfg(feature = "injective")]
    pub const TYPE_URL: &'static str = "/injective.tokenfactory.v1beta1.MsgBurn";
    #[cfg(feature = "sei")]
    pub const TYPE_URL: &'static str = "/seiprotocol.seichain.tokenfactory.v1beta1.MsgBurn";
}

impl TryFrom<Binary> for MsgBurn {
    type Error = StdError;
    fn try_from(binary: Binary) -> Result<Self, Self::Error> {
        Self::decode(binary.as_slice()).map_err(|e| {
            StdError::generic_err(format!(
                "MsgBurn Unable to decode binary: \n  - base64: {}\n  - bytes array: {:?}\n\n{:?}",
                binary,
                binary.to_vec(),
                e
            ))
        })
    }
}

#[cfg(not(any(feature = "injective", feature = "sei")))]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MsgMint {
    #[prost(string, tag = "1")]
    pub sender: ::prost::alloc::string::String,
    #[prost(message, optional, tag = "2")]
    pub amount: ::core::option::Option<cosmos_sdk_proto::cosmos::base::v1beta1::Coin>,
    #[prost(string, tag = "3")]
    pub mint_to_address: ::prost::alloc::string::String,
}

#[cfg(feature = "injective")]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MsgMint {
    #[prost(string, tag = "1")]
    pub sender: ::prost::alloc::string::String,
    #[prost(message, optional, tag = "2")]
    pub amount: ::core::option::Option<cosmos_sdk_proto::cosmos::base::v1beta1::Coin>,
}

#[cfg(feature = "sei")]
#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MsgMint {
    #[prost(message, optional, tag = "2")]
    pub amount: ::core::option::Option<cosmos_sdk_proto::cosmos::base::v1beta1::Coin>,
}

impl MsgMint {
    #[cfg(not(any(feature = "injective", feature = "sei")))]
    pub const TYPE_URL: &'static str = "/osmosis.tokenfactory.v1beta1.MsgMint";
    #[cfg(feature = "injective")]
    pub const TYPE_URL: &'static str = "/injective.tokenfactory.v1beta1.MsgMint";
    #[cfg(feature = "sei")]
    pub const TYPE_URL: &'static str = "/seiprotocol.seichain.tokenfactory.v1beta1.MsgMint";
}

impl TryFrom<Binary> for MsgMint {
    type Error = StdError;
    fn try_from(binary: Binary) -> Result<Self, Self::Error> {
        Self::decode(binary.as_slice()).map_err(|e| {
            StdError::generic_err(format!(
                "MsgMint Unable to decode binary: \n  - base64: {}\n  - bytes array: {:?}\n\n{:?}",
                binary,
                binary.to_vec(),
                e
            ))
        })
    }
}

#[derive(Clone, PartialEq, ::prost::Message)]
pub struct MsgSetBeforeSendHook {
    #[prost(string, tag = "1")]
    pub sender: ::prost::alloc::string::String,
    #[prost(string, tag = "2")]
    pub denom: ::prost::alloc::string::String,
    #[prost(string, tag = "3")]
    pub cosmwasm_address: ::prost::alloc::string::String,
}

impl MsgSetBeforeSendHook {
    pub const TYPE_URL: &'static str = "/osmosis.tokenfactory.v1beta1.MsgSetBeforeSendHook";
}

impl TryFrom<Binary> for MsgSetBeforeSendHook {
    type Error = StdError;
    fn try_from(binary: Binary) -> Result<Self, Self::Error> {
        Self::decode(binary.as_slice()).map_err(|e| {
            StdError::generic_err(format!(
                "MsgSetBeforeSendHook Unable to decode binary: \n  - base64: {}\n  - bytes array: {:?}\n\n{:?}",
                binary,
                binary.to_vec(),
                e
            ))
        })
    }
}

pub fn tf_create_denom_msg<T>(sender: impl Into<String>, denom: impl Into<String>) -> CosmosMsg<T>
where
    T: CustomMsg,
{
    #[cfg(not(any(feature = "sei")))]
    let create_denom_msg = MsgCreateDenom {
        sender: sender.into(),
        subdenom: denom.into(),
    };

    #[cfg(feature = "sei")]
    let create_denom_msg = MsgCreateDenom {
        subdenom: denom.into(),
    };

    CosmosMsg::Stargate {
        type_url: MsgCreateDenom::TYPE_URL.to_string(),
        value: Binary::from(create_denom_msg.encode_to_vec()),
    }
}

pub fn tf_mint_msg<T>(
    sender: impl Into<String>,
    coin: Coin,
    receiver: impl Into<String>,
) -> Vec<CosmosMsg<T>>
where
    T: CustomMsg,
{
    let sender_addr: String = sender.into();
    let receiver_addr: String = receiver.into();

    #[cfg(not(any(feature = "injective", feature = "sei")))]
    let mint_msg = MsgMint {
        sender: sender_addr.clone(),
        amount: Some(ProtoCoin {
            denom: coin.denom.to_string(),
            amount: coin.amount.to_string(),
        }),
        mint_to_address: receiver_addr.clone(),
    };

    #[cfg(feature = "injective")]
    let mint_msg = MsgMint {
        sender: sender_addr.clone(),
        amount: Some(ProtoCoin {
            denom: coin.denom.to_string(),
            amount: coin.amount.to_string(),
        }),
    };

    #[cfg(feature = "sei")]
    let mint_msg = MsgMint {
        amount: Some(ProtoCoin {
            denom: coin.denom.to_string(),
            amount: coin.amount.to_string(),
        }),
    };

    #[cfg(not(any(feature = "injective", feature = "sei")))]
    return vec![CosmosMsg::Stargate {
        type_url: MsgMint::TYPE_URL.to_string(),
        value: Binary::from(mint_msg.encode_to_vec()),
    }];

    #[cfg(any(feature = "injective", feature = "sei"))]
    if sender_addr == receiver_addr {
        vec![CosmosMsg::Stargate {
            type_url: MsgMint::TYPE_URL.to_string(),
            value: Binary::from(mint_msg.encode_to_vec()),
        }]
    } else {
        vec![
            CosmosMsg::Stargate {
                type_url: MsgMint::TYPE_URL.to_string(),
                value: Binary::from(mint_msg.encode_to_vec()),
            },
            BankMsg::Send {
                to_address: receiver_addr,
                amount: vec![coin],
            }
            .into(),
        ]
    }
}

pub fn tf_burn_msg<T>(sender: impl Into<String>, coin: Coin) -> CosmosMsg<T>
where
    T: CustomMsg,
{
    #[cfg(not(any(feature = "injective", feature = "sei")))]
    let burn_msg = MsgBurn {
        sender: sender.into(),
        amount: Some(ProtoCoin {
            denom: coin.denom,
            amount: coin.amount.to_string(),
        }),
        burn_from_address: "".to_string(),
    };

    #[cfg(feature = "injective")]
    let burn_msg = MsgBurn {
        sender: sender.into(),
        amount: Some(ProtoCoin {
            denom: coin.denom,
            amount: coin.amount.to_string(),
        }),
    };

    #[cfg(feature = "sei")]
    let burn_msg = MsgBurn {
        amount: Some(ProtoCoin {
            denom: coin.denom,
            amount: coin.amount.to_string(),
        }),
    };

    CosmosMsg::Stargate {
        type_url: MsgBurn::TYPE_URL.to_string(),
        value: Binary::from(burn_msg.encode_to_vec()),
    }
}

pub fn tf_before_send_hook_msg<T>(
    sender: impl Into<String>,
    denom: impl Into<String>,
    cosmwasm_address: impl Into<String>,
) -> CosmosMsg<T>
where
    T: CustomMsg,
{
    let msg = MsgSetBeforeSendHook {
        sender: sender.into(),
        denom: denom.into(),
        cosmwasm_address: cosmwasm_address.into(),
    };

    CosmosMsg::Stargate {
        type_url: MsgSetBeforeSendHook::TYPE_URL.to_string(),
        value: Binary::from(msg.encode_to_vec()),
    }
}
