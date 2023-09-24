use std::borrow::Borrow;
use std::cell::{Cell, UnsafeCell};
use std::collections::hash_map::RandomState;
use std::hash::{BuildHasher, Hash};
use std::iter::FromIterator;
use std::ops::Index;

use indexmap::IndexSet;
use stable_deref_trait::StableDeref;

#[cfg(feature = "serde")]
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Append-only version of `indexmap::IndexSet` where
/// insertion does not require mutable access
#[derive(Debug)]
pub struct FrozenIndexSet<T, S = RandomState> {
    set: UnsafeCell<IndexSet<T, S>>,
    /// Eq/Hash implementations can have side-effects, and using Rc it is possible
    /// for FrozenIndexSet::insert to be called on a key that itself contains the same
    /// `FrozenIndexSet`, whose `eq` implementation also calls FrozenIndexSet::insert
    ///
    /// We use this `in_use` flag to guard against any reentrancy.
    in_use: Cell<bool>,
}

// safety: UnsafeCell implies !Sync

impl<T: Eq + Hash> FrozenIndexSet<T> {
    pub fn new() -> Self {
        Self::from(IndexSet::new())
    }
}

impl<T: Eq + Hash + StableDeref, S: BuildHasher> FrozenIndexSet<T, S> {
    // these should never return &T
    // these should never delete any entries
    pub fn insert(&self, value: T) -> &T::Target {
        assert!(!self.in_use.get());
        self.in_use.set(true);
        let ret = unsafe {
            let set = self.set.get();
            let (index, _was_vacant) = (*set).insert_full(value);
            &*(*set)[index]
        };
        self.in_use.set(false);
        ret
    }

    // these should never return &T
    // these should never delete any entries
    pub fn insert_full(&self, value: T) -> (usize, &T::Target) {
        assert!(!self.in_use.get());
        self.in_use.set(true);
        let ret = unsafe {
            let set = self.set.get();
            let (index, _was_vacant) = (*set).insert_full(value);
            (index, &*(*set)[index])
        };
        self.in_use.set(false);
        ret
    }

    // TODO implement in case the standard Entry API gets improved
    // // TODO avoid double lookup
    // pub fn entry<Q: ?Sized>(&self, value: &Q) -> Entry<T, Q>
    //     where Q: Hash + Equivalent<T> + ToOwned<Owned = T>
    // {
    //     assert!(!self.in_use.get());
    //     self.in_use.set(true);
    //     unsafe {
    //         let set = self.set.get();
    //         match (*set).get_full(value) {
    //             Some((index, reference)) => {
    //                 Entry::Occupied(OccupiedEntry {
    //                     index,
    //                     reference,
    //                     set: &*set,
    //                 })
    //             }
    //             None => {
    //                 Entry::Vacant(VacantEntry {
    //                     value: Cow::Borrowed(value),
    //                     set: &*set,
    //                 })
    //             }
    //         }
    //     }
    // }

    pub fn get<Q: ?Sized>(&self, k: &Q) -> Option<&T::Target>
    where
        T: Borrow<Q>,
        Q: Hash + Eq,
    {
        assert!(!self.in_use.get());
        self.in_use.set(true);
        let ret = unsafe {
            let set = self.set.get();
            (*set).get(k).map(|x| &**x)
        };
        self.in_use.set(false);
        ret
    }

    pub fn get_full<Q: ?Sized>(&self, k: &Q) -> Option<(usize, &T::Target)>
    where
        T: Borrow<Q>,
        Q: Hash + Eq,
    {
        assert!(!self.in_use.get());
        self.in_use.set(true);
        let ret = unsafe {
            let set = self.set.get();
            (*set).get_full(k).map(|(i, x)| (i, &**x))
        };
        self.in_use.set(false);
        ret
    }

    pub fn get_index(&self, index: usize) -> Option<&T::Target> {
        assert!(!self.in_use.get());
        self.in_use.set(true);
        let ret = unsafe {
            let set = self.set.get();
            (*set).get_index(index).map(|r| &**r)
        };
        self.in_use.set(false);
        ret
    }
}

impl<T, S> FrozenIndexSet<T, S> {
    pub fn into_set(self) -> IndexSet<T, S> {
        self.set.into_inner()
    }

    /// Get mutable access to the underlying [`IndexSet`].
    ///
    /// This is safe, as it requires a `&mut self`, ensuring nothing is using
    /// the 'frozen' contents.
    pub fn as_mut(&mut self) -> &mut IndexSet<T, S> {
        unsafe { &mut *self.set.get() }
    }

    // TODO add more
}

impl<T, S> From<IndexSet<T, S>> for FrozenIndexSet<T, S> {
    fn from(set: IndexSet<T, S>) -> Self {
        Self {
            set: UnsafeCell::new(set),
            in_use: Cell::new(false),
        }
    }
}

impl<T: Eq + Hash + StableDeref, S> Index<usize> for FrozenIndexSet<T, S> {
    type Output = T::Target;
    fn index(&self, idx: usize) -> &T::Target {
        assert!(!self.in_use.get());
        self.in_use.set(true);
        let ret = unsafe {
            let set = self.set.get();
            &*(*set)[idx]
        };
        self.in_use.set(false);
        ret
    }
}

impl<T: Eq + Hash, S: Default + BuildHasher> FromIterator<T> for FrozenIndexSet<T, S> {
    fn from_iter<U>(iter: U) -> Self
    where
        U: IntoIterator<Item = T>,
    {
        let set: IndexSet<_, _> = iter.into_iter().collect();
        set.into()
    }
}

impl<T: Eq + Hash, S: Default> Default for FrozenIndexSet<T, S> {
    fn default() -> Self {
        Self::from(IndexSet::default())
    }
}

impl<T: Hash + Eq, S: BuildHasher> PartialEq for FrozenIndexSet<T, S> {
    fn eq(&self, other: &Self) -> bool {
        assert!(!self.in_use.get());
        assert!(!other.in_use.get());
        self.in_use.set(true);
        other.in_use.set(true);
        let ret = unsafe { *self.set.get() == *other.set.get() };
        self.in_use.set(false);
        other.in_use.set(false);
        ret
    }
}

impl<K: Clone, V: Clone> Clone for FrozenIndexSet<K, V> {
    fn clone(&self) -> Self {
        assert!(!self.in_use.get());
        self.in_use.set(true);
        let self_clone = Self {
            set: unsafe { self.set.get().as_ref().unwrap() }.clone().into(),
            in_use: Cell::from(false),
        };
        self.in_use.set(false);
        return self_clone;
    }
}

#[cfg(feature = "serde")]
impl<T, S> Serialize for FrozenIndexSet<T, S>
where
    T: Eq + Hash + Serialize,
    S: BuildHasher,
{
    fn serialize<Ser>(&self, serializer: Ser) -> Result<Ser::Ok, Ser::Error>
    where
        Ser: Serializer,
    {
        assert!(!self.in_use.get());
        self.in_use.set(true);
        let map_serialized = unsafe { self.set.get().as_ref().unwrap() }.serialize(serializer);
        self.in_use.set(false);
        return map_serialized;
    }
}

#[cfg(feature = "serde")]
impl<'de, K, S> Deserialize<'de> for FrozenIndexSet<K, S>
where
    K: Deserialize<'de> + Eq + Hash,
    S: BuildHasher + Default,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        IndexSet::deserialize(deserializer).map(FrozenIndexSet::from)
    }
}
