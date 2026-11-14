use rvim_types::{Arena, Id};

/// A map facade over Arena for code that needs stable externally supplied IDs.
/// Iteration is sorted by raw ID so window/tab order remains creation order for
/// monotonically allocated IDs.
#[derive(Clone, Debug)]
pub struct ArenaMap<I: Id, T> {
    arena: Arena<T>,
    marker: std::marker::PhantomData<I>,
}

impl<I: Id, T> Default for ArenaMap<I, T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<I: Id, T> ArenaMap<I, T> {
    pub fn new() -> Self {
        Self {
            arena: Arena::new(),
            marker: std::marker::PhantomData,
        }
    }

    pub fn len(&self) -> usize {
        self.arena.len()
    }

    pub fn is_empty(&self) -> bool {
        self.arena.is_empty()
    }

    pub fn insert(&mut self, id: I, value: T) -> Option<T> {
        self.arena.insert_at(id, value)
    }

    pub fn get(&self, id: &I) -> Option<&T> {
        self.arena.get(*id)
    }

    pub fn get_mut(&mut self, id: &I) -> Option<&mut T> {
        self.arena.get_mut(*id)
    }

    pub fn remove(&mut self, id: &I) -> Option<T> {
        self.arena.remove(*id)
    }

    pub fn contains_key(&self, id: &I) -> bool {
        self.arena.contains(*id)
    }

    pub fn keys(&self) -> impl Iterator<Item = I> + '_ {
        let mut ids: Vec<I> = self.arena.ids().collect();
        ids.sort_by_key(|id| id.raw());
        ids.into_iter()
    }

    pub fn values(&self) -> impl Iterator<Item = &T> + '_ {
        self.arena.values()
    }

    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut T> + '_ {
        self.arena.values_mut()
    }

    pub fn iter(&self) -> impl Iterator<Item = (I, &T)> + '_ {
        self.arena
            .iter()
            .map(|(raw, value)| (I::from_raw(raw), value))
    }

    pub fn clear(&mut self) {
        self.arena.clear();
    }
}
