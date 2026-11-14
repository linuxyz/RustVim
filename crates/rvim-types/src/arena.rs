use crate::ids::Id;
use std::collections::HashMap;

#[derive(Clone, Debug)]
struct Slot<T> {
    generation: u32,
    value: Option<T>,
}

/// A small generational arena with O(1) insert/get/remove.
///
/// Values are addressed by typed [`Id`] newtypes. Removal bumps the slot
/// generation, so a stale ID can never observe a reused slot.
#[derive(Clone, Debug, Default)]
pub struct Arena<T> {
    slots: Vec<Slot<T>>,
    free: Vec<u32>,
    live: usize,
}

impl<T> Arena<T> {
    pub fn new() -> Self {
        Self {
            slots: Vec::new(),
            free: Vec::new(),
            live: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.live
    }

    pub fn is_empty(&self) -> bool {
        self.live == 0
    }

    pub fn insert<I: Id>(&mut self, value: T) -> I {
        if let Some(index) = self.free.pop() {
            let slot = &mut self.slots[index as usize];
            slot.value = Some(value);
            self.live += 1;
            return I::from_raw(((slot.generation as u64) << 32) | index as u64);
        }
        let index = self.slots.len() as u32;
        debug_assert!(index < u32::MAX);
        self.slots.push(Slot {
            generation: 1,
            value: Some(value),
        });
        self.live += 1;
        I::from_raw((1u64 << 32) | index as u64)
    }

    pub fn insert_at<I: Id>(&mut self, id: I, value: T) -> Option<T> {
        let raw = id.raw();
        let index = (raw as u32) as usize;
        let generation = (raw >> 32) as u32;
        while self.slots.len() <= index {
            self.slots.push(Slot {
                generation,
                value: None,
            });
        }
        if let Some(position) = self.free.iter().position(|free| *free as usize == index) {
            self.free.swap_remove(position);
        }
        let slot = &mut self.slots[index];
        let previous = slot.value.replace(value);
        slot.generation = generation;
        if previous.is_none() {
            self.live += 1;
        }
        previous
    }

    pub fn get<I: Id>(&self, id: I) -> Option<&T> {
        let raw = id.raw();
        let index = raw as u32;
        let generation = (raw >> 32) as u32;
        self.slots.get(index as usize).and_then(|slot| {
            if slot.generation == generation {
                slot.value.as_ref()
            } else {
                None
            }
        })
    }

    pub fn get_mut<I: Id>(&mut self, id: I) -> Option<&mut T> {
        let raw = id.raw();
        let index = raw as u32;
        let generation = (raw >> 32) as u32;
        self.slots.get_mut(index as usize).and_then(|slot| {
            if slot.generation == generation {
                slot.value.as_mut()
            } else {
                None
            }
        })
    }

    pub fn contains<I: Id>(&self, id: I) -> bool {
        self.get(id).is_some()
    }

    pub fn remove<I: Id>(&mut self, id: I) -> Option<T> {
        let raw = id.raw();
        let index = raw as u32;
        let generation = (raw >> 32) as u32;
        let slot = self.slots.get_mut(index as usize)?;
        if slot.generation != generation {
            return None;
        }
        let value = slot.value.take()?;
        slot.generation = slot.generation.wrapping_add(1).max(1);
        self.free.push(index);
        self.live -= 1;
        Some(value)
    }

    pub fn iter(&self) -> impl Iterator<Item = (u64, &T)> + '_ {
        self.slots.iter().enumerate().filter_map(|(index, slot)| {
            slot.value
                .as_ref()
                .map(|value| (((slot.generation as u64) << 32) | index as u64, value))
        })
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (u64, &mut T)> + '_ {
        self.slots
            .iter_mut()
            .enumerate()
            .filter_map(|(index, slot)| {
                let generation = slot.generation;
                slot.value
                    .as_mut()
                    .map(|value| (((generation as u64) << 32) | index as u64, value))
            })
    }

    pub fn ids<I: Id>(&self) -> impl Iterator<Item = I> + '_ {
        self.iter().map(|(raw, _)| I::from_raw(raw))
    }

    pub fn values(&self) -> impl Iterator<Item = &T> + '_ {
        self.iter().map(|(_, value)| value)
    }

    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut T> + '_ {
        self.iter_mut().map(|(_, value)| value)
    }

    pub fn clear(&mut self) {
        self.slots.clear();
        self.free.clear();
        self.live = 0;
    }
}

/// A string-keyed secondary index over arena values.
#[derive(Clone, Debug, Default)]
pub struct NameIndex<I: Id> {
    names: HashMap<String, I>,
}

impl<I: Id> NameIndex<I> {
    pub fn insert(&mut self, name: impl Into<String>, id: I) {
        self.names.insert(name.into(), id);
    }

    pub fn get(&self, name: &str) -> Option<I> {
        self.names.get(name).copied()
    }

    pub fn remove(&mut self, name: &str) -> Option<I> {
        self.names.remove(name)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, I)> + '_ {
        self.names.iter().map(|(name, id)| (name.as_str(), *id))
    }
}
