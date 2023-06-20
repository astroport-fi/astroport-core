//! Circular buffer which is built over [`Item`] and [`Map`].
//! Might be useful to store time series data in contracts.
//!
//! # Example
//! ```
//! use cosmwasm_std::testing::MockStorage;
//! use astroport_circular_buffer::{BufferManager, CircularBuffer};
//!
//! const CIRCULAR_BUFFER: CircularBuffer<u128> = CircularBuffer::new("buffer_state", "buffer");
//!
//! let mut store = MockStorage::new();
//! BufferManager::init(&mut store, CIRCULAR_BUFFER, 10).unwrap();
//! let mut buffer = BufferManager::new(&store, CIRCULAR_BUFFER).unwrap();
//!
//! let data = (1..=10u128).collect::<Vec<_>>();
//! buffer.push_many(&data);
//! buffer.commit(&mut store).unwrap();
//!
//! let values = buffer.read(&store, 0u32..=9, true).unwrap();
//! let all_values = buffer.read_all(&store).unwrap();
//! ```

use std::collections::HashMap;
use std::fmt::{Debug, Display, Formatter};
use std::marker::PhantomData;

use cosmwasm_schema::cw_serde;
use cosmwasm_schema::serde::de::DeserializeOwned;
use cosmwasm_schema::serde::Serialize;
use cosmwasm_std::{StdError, Storage};
use cw_storage_plus::{Item, Map};

use crate::error::{BufferError, BufferResult};

pub mod error;

#[cw_serde]
pub struct BufferState {
    capacity: u32,
    head: u32,
}

pub struct CircularBuffer<'a, V> {
    state_key: &'a str,
    array_namespace: &'a str,
    data_type: PhantomData<V>,
}

impl<'a, V> CircularBuffer<'a, V> {
    pub const fn new(state_key: &'a str, array_namespace: &'a str) -> Self {
        Self {
            state_key,
            array_namespace,
            data_type: PhantomData,
        }
    }

    pub const fn state(&'a self) -> Item<BufferState> {
        Item::new(self.state_key)
    }

    pub const fn array(&'a self) -> Map<u32, V> {
        Map::new(self.array_namespace)
    }
}

pub struct BufferManager<'a, V> {
    state: BufferState,
    store_iface: CircularBuffer<'a, V>,
    precommit_buffer: HashMap<u32, &'a V>,
}

impl<'a, V> BufferManager<'a, V>
where
    V: Serialize + DeserializeOwned + 'a,
{
    /// Static function to initialize buffer in storage.
    /// Intended to be called during contract initialization.
    pub fn init(
        store: &mut dyn Storage,
        store_iface: CircularBuffer<'a, V>,
        capacity: u32,
    ) -> BufferResult<()> {
        let state_iface = store_iface.state();

        if state_iface.may_load(store)?.is_some() {
            return Err(BufferError::BufferAlreadyInitialized {});
        }

        state_iface.save(store, &BufferState { capacity, head: 0 })?;

        Ok(())
    }

    /// Initialize buffer manager.
    /// In case buffer is not initialized it throws [`BufferError::BufferNotInitialized`] error.
    pub fn new(store: &dyn Storage, store_iface: CircularBuffer<'a, V>) -> BufferResult<Self> {
        Ok(Self {
            state: store_iface.state().load(store).map_err(|err| {
                if let StdError::NotFound { .. } = err {
                    BufferError::BufferNotInitialized {}
                } else {
                    err.into()
                }
            })?,
            store_iface,
            precommit_buffer: HashMap::new(),
        })
    }

    /// Returns current buffer capacity.
    pub fn capacity(&self) -> u32 {
        self.state.capacity
    }

    /// Returns current buffer head.
    pub fn head(&self) -> u32 {
        self.state.head
    }

    /// Push value to precommit buffer.
    pub fn push(&mut self, value: &'a V) {
        self.precommit_buffer.insert(self.state.head, value);
        self.state.head = (self.state.head + 1) % self.state.capacity;
    }

    /// Push multiple values to precommit buffer.
    pub fn push_many(&mut self, values: &'a [V]) {
        for value in values {
            self.push(value);
        }
    }

    /// Push value to precommit buffer and commit it to storage.
    pub fn instant_push(&mut self, store: &mut dyn Storage, value: &'a V) -> BufferResult<()> {
        self.push(value);
        self.commit(store)
    }

    /// Commit in storage current state and precommit buffer. Buffer is erased after commit.
    pub fn commit(&mut self, store: &mut dyn Storage) -> BufferResult<()> {
        let array_key = self.store_iface.array();
        for (&key, value) in &self.precommit_buffer {
            if key >= self.state.capacity {
                return Err(BufferError::SaveValueError(key));
            }
            array_key.save(store, key, value)?;
        }
        self.precommit_buffer.clear();
        self.store_iface.state().save(store, &self.state)?;

        Ok(())
    }

    /// Read values from storage by indexes. If `stop_if_empty` is true,
    /// reading will stop when first empty value is encountered.
    /// Otherwise, [`BufferError::IndexNotFound`] error will be thrown.
    ///
    /// ## Examples:
    /// ```
    /// # use cosmwasm_std::{testing::MockStorage};
    /// # use astroport_circular_buffer::{BufferManager, CircularBuffer};
    /// # let mut store = MockStorage::new();
    /// # const CIRCULAR_BUFFER: CircularBuffer<u128> = CircularBuffer::new("buffer_state", "buffer");
    /// # BufferManager::init(&mut store, CIRCULAR_BUFFER, 10).unwrap();
    /// # let mut buffer = BufferManager::new(&store, CIRCULAR_BUFFER).unwrap();
    /// # let data = (1..=10u128).collect::<Vec<_>>();
    /// # buffer.push_many(&data);
    /// # buffer.commit(&mut store).unwrap();
    ///
    /// let values = buffer.read(&store, 0u32..=9, false).unwrap();
    /// let values = buffer.read(&store, vec![0u32, 5, 7], false).unwrap();
    /// let values = buffer.read(&store, (0u32..buffer.capacity()).step_by(2), false).unwrap();
    /// ```
    pub fn read(
        &self,
        store: &dyn Storage,
        indexes: impl IntoIterator<Item = impl Into<u32> + Display>,
        stop_if_empty: bool,
    ) -> BufferResult<Vec<V>> {
        let array_key = self.store_iface.array();
        let mut values = vec![];
        for index in indexes {
            let ind = index.into();
            if ind > self.state.capacity - 1 {
                return Err(BufferError::ReadAheadError(ind));
            } else {
                let value = array_key.load(store, ind).map_err(|err| {
                    if let StdError::NotFound { .. } = err {
                        BufferError::IndexNotFound(ind)
                    } else {
                        err.into()
                    }
                });
                match value {
                    Ok(value) => values.push(value),
                    Err(BufferError::IndexNotFound(_)) if stop_if_empty => return Ok(values),
                    Err(err) => return Err(err),
                }
            }
        }

        Ok(values)
    }

    /// Read all available values from storage.
    pub fn read_all(&self, store: &dyn Storage) -> BufferResult<Vec<V>> {
        self.read(store, 0..self.state.capacity, true)
    }

    /// Read last saved value from storage. Returns None if buffer is empty.
    pub fn read_last(&self, store: &dyn Storage) -> BufferResult<Option<V>> {
        self.read_single(
            store,
            (self.state.capacity + self.state.head - 1) % self.state.capacity,
        )
    }

    /// Looped read. Returns None if value in buffer does not exist.
    pub fn read_single(
        &self,
        store: &dyn Storage,
        index: impl Into<u32>,
    ) -> BufferResult<Option<V>> {
        let ind = index.into() % self.state.capacity;
        let res = self.store_iface.array().load(store, ind);
        if let Err(StdError::NotFound { .. }) = res {
            Ok(None)
        } else {
            res.map(Some).map_err(Into::into)
        }
    }

    /// This operation is gas consuming. However, it might be helpful in rare cases.
    pub fn clear_buffer(&self, store: &mut dyn Storage) {
        let array_key = self.store_iface.array();
        (0..self.state.capacity).for_each(|i| array_key.remove(store, i))
    }

    /// Whether index exists in buffer.
    pub fn exists(&self, store: &dyn Storage, index: u32) -> bool {
        self.store_iface
            .array()
            .has(store, index % self.state.capacity)
    }
}

impl<V: Debug> Debug for BufferManager<'_, V> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BufferManager")
            .field("state", &self.state)
            .field("precommit_buffer", &self.precommit_buffer)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use cosmwasm_std::testing::MockStorage;
    use cosmwasm_std::Uint128;

    use super::*;

    type DataType = Uint128;
    const CIRCULAR_BUFFER: CircularBuffer<DataType> = CircularBuffer::new("buffer_state", "buffer");

    #[test]
    fn test_single_push() {
        let mut store = MockStorage::new();

        BufferManager::init(&mut store, CIRCULAR_BUFFER, 10).unwrap();

        // Buffer can be initialized only once
        let err = BufferManager::init(&mut store, CIRCULAR_BUFFER, 10).unwrap_err();
        assert_eq!(err, BufferError::BufferAlreadyInitialized {});

        let mut buffer = BufferManager::new(&store, CIRCULAR_BUFFER).unwrap();

        assert_eq!(buffer.capacity(), 10);
        assert_eq!(
            format!("{:?}", &buffer),
            "BufferManager { state: BufferState { capacity: 10, head: 0 }, precommit_buffer: {} }"
        );

        let data = (1..=15u8).map(DataType::from).collect::<Vec<_>>();
        data.iter().for_each(|i| buffer.push(i));
        buffer.commit(&mut store).unwrap();

        // read last saved value
        let head = buffer.read_last(&store).unwrap().unwrap();
        assert_eq!(head.u128(), 15);

        let saved = buffer
            .read(&store, 0u32..=9, true)
            .unwrap()
            .into_iter()
            .map(|i| i.u128())
            .collect::<Vec<_>>();
        assert_eq!(saved, vec![11, 12, 13, 14, 15, 6, 7, 8, 9, 10]);

        // check instant push
        let val = DataType::from(16u128);
        buffer.instant_push(&mut store, &val).unwrap();
        let saved = buffer
            .read(&store, 0u32..=9, true)
            .unwrap()
            .into_iter()
            .map(|i| i.u128())
            .collect::<Vec<_>>();
        assert_eq!(saved, vec![11, 12, 13, 14, 15, 16, 7, 8, 9, 10]);

        // read invalid index
        let err = buffer.read(&store, [10u32, 11u32], true).unwrap_err();
        assert_eq!(err, BufferError::ReadAheadError(10));
    }

    #[test]
    fn test_push_many() {
        let mut store = MockStorage::new();

        // Trying to create buffer manager before initialization
        let err = BufferManager::new(&store, CIRCULAR_BUFFER).unwrap_err();
        assert_eq!(err, BufferError::BufferNotInitialized {});

        BufferManager::init(&mut store, CIRCULAR_BUFFER, 10).unwrap();

        let mut buffer = BufferManager::new(&store, CIRCULAR_BUFFER).unwrap();

        // read empty buffer
        let err = buffer.read(&store, [0u8], false).unwrap_err();
        assert_eq!(err, BufferError::IndexNotFound(0));

        let data = (1..=15u8).map(DataType::from).collect::<Vec<_>>();
        buffer.push_many(&data);
        buffer.commit(&mut store).unwrap();

        let saved = buffer
            .read_all(&store)
            .unwrap()
            .into_iter()
            .map(|i| i.u128())
            .collect::<Vec<_>>();
        assert_eq!(saved, vec![11, 12, 13, 14, 15, 6, 7, 8, 9, 10]);

        let partial_read = buffer
            .read(&store, (0u32..buffer.capacity()).step_by(2), true)
            .unwrap()
            .into_iter()
            .map(|i| i.u128())
            .collect::<Vec<_>>();
        assert_eq!(partial_read, vec![11, 13, 15, 7, 9]);
    }
}
