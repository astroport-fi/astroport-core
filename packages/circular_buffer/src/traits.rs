use crate::error::BufferError;
use cosmwasm_std::{Storage, Uint128};

/// For those [`BufferManager`] implementations which require resizing logic
/// it is required to implement this trait.
/// You must be very careful while implementing resizing logic,
/// because it can lead to data inconsistency.
pub trait ResizableCircularBuffer {
    type Error: Into<BufferError>;
    fn pre_resize_hook(
        &mut self,
        store: &mut dyn Storage,
        new_capacity: u32,
    ) -> Result<(), Self::Error>;
    fn post_resize_hook(
        &mut self,
        store: &mut dyn Storage,
        capacity: u32,
    ) -> Result<(), Self::Error>;
}

// Generic implementation of [`ResizableCircularBuffer`] for [`BufferManager`] with [`Uint128`]
// values, which can be used as an example for custom implementations.
impl<'a> ResizableCircularBuffer for crate::BufferManager<'a, Uint128> {
    type Error = BufferError;

    /// Invalidate all keys which are greater or equal than new capacity.  
    /// Warning: this function may be gas intensive and fail due to gas limit if new capacity is
    /// way lower than old one.
    fn pre_resize_hook(
        &mut self,
        store: &mut dyn Storage,
        new_capacity: u32,
    ) -> Result<(), Self::Error> {
        let array_key = self.store_iface.array();
        (new_capacity..self.state.capacity - 1).for_each(|i| array_key.remove(store, i));

        Ok(())
    }

    fn post_resize_hook(
        &mut self,
        _store: &mut dyn Storage,
        _capacity: u32,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
}
