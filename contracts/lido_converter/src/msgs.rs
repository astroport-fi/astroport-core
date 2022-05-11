// SPDX-License-Identifier: GPL-3.0-only
// Copyright Astroport
// Copyright Lido

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// ## Description
/// This structure describes the basic settings for creating a contract.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    /// the Lido Terra token addresses
    pub stluna_address: String,
    pub bluna_address: String,

    /// the Lido Terra Hub address
    pub hub_address: String,
}
