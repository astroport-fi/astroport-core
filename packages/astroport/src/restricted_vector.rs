use crate::DecimalCheckedOps;
use cosmwasm_std::{Decimal, StdError, StdResult, Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt::Display;

/// Vec wrapper for internal use.
/// Some business logic relies on an order of this vector, thus it is forbidden to sort it
/// or remove elements. New values can be added using .update() ONLY.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct RestrictedVector<K, V>(Vec<(K, V)>);

pub trait Increaseable
where
    Self: Sized,
{
    fn increase(self, new: Self) -> StdResult<Self>;
}

impl<K, V> RestrictedVector<K, V>
where
    K: Clone + PartialEq + Display,
    V: Copy + Increaseable,
{
    pub fn new(key: K, value: V) -> Self {
        Self(vec![(key, value)])
    }

    pub fn get_last(&self, key: &K) -> StdResult<V> {
        self.0
            .last()
            .filter(|(k, _)| k == key)
            .map(|(_, v)| v)
            .cloned()
            .ok_or_else(|| StdError::generic_err(format!("Key {} not found", key)))
    }

    pub fn update(&mut self, key: &K, value: V) -> StdResult<V> {
        let found = self.0.iter_mut().find(|(k, _)| k == key);
        let r = match found {
            Some((_, v)) => {
                *v = v.increase(value)?;
                *v
            }
            None => {
                self.0.push((key.clone(), value));
                value
            }
        };

        Ok(r)
    }

    pub fn load(&self, key: &K) -> Option<V> {
        self.0
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, value)| *value)
    }

    pub fn inner_ref(&self) -> &Vec<(K, V)> {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl Increaseable for Decimal {
    fn increase(self, new: Decimal) -> StdResult<Decimal> {
        self.checked_add(new).map_err(Into::into)
    }
}

impl Increaseable for Uint128 {
    fn increase(self, new: Uint128) -> StdResult<Uint128> {
        self.checked_add(new).map_err(Into::into)
    }
}

impl<K, V> Default for RestrictedVector<K, V> {
    fn default() -> Self {
        Self(vec![])
    }
}

impl<K, V> From<Vec<(K, V)>> for RestrictedVector<K, V> {
    fn from(v: Vec<(K, V)>) -> Self {
        Self(v)
    }
}
