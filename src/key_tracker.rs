use std::ops::{BitOr, Index, IndexMut};

use itertools::Itertools;
use midly::num::{u4, u7};

use crate::key_effect::KeyEffect;

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash)]
pub struct ChannelSet(u16);
impl ChannelSet {
    pub fn set_on(&mut self, channel: u4) {
        self.0 |= 1 << channel.as_int();
    }
    pub fn set_off(&mut self, channel: u4) {
        self.0 &= !(1 << channel.as_int())
    }
    pub fn any(self) -> bool {
        self.0 != 0
    }
}

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash)]
pub struct KeyStatus {
    /// Channels on which user is pressing the key.
    pub input: ChannelSet,
    /// Channels on which we are recording a press of the key.
    ///
    /// This is usually the same as `input`, except after stopping a recording,
    /// when it contains only the keys that have been held since the recording
    /// was stopped.
    pub recording: ChannelSet,
    /// Most recent velocity with which the key was pressed (for resumption).
    pub last_velocity: u7,
}

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash)]
pub struct KeySet([u64; 2]);
impl KeySet {
    pub fn new() -> Self {
        Self::default()
    }
    fn split_index(index: u7) -> (usize, u64) {
        let array_index = index.as_int() as usize >> 6;
        let bitmask = 1 << (index.as_int() & 0x3F);
        (array_index, bitmask)
    }
    pub fn contains(self, key: u7) -> bool {
        let (array_index, bitmask) = Self::split_index(key);
        self.0[array_index] & bitmask != 0
    }
    pub fn update(&mut self, key_effect: impl Into<KeyEffect>) {
        match key_effect.into() {
            KeyEffect::Press { key, vel: _ } => {
                self.insert(key);
            }
            KeyEffect::Release { key } => {
                self.remove(key);
            }
            _ => (),
        }
    }
    pub fn insert(&mut self, key: u7) -> bool {
        let ret = !self.contains(key);
        let (array_index, bitmask) = Self::split_index(key);
        self.0[array_index] |= bitmask;
        ret
    }
    pub fn remove(&mut self, key: u7) -> bool {
        let ret = self.contains(key);
        let (array_index, bitmask) = Self::split_index(key);
        self.0[array_index] &= !bitmask;
        ret
    }
    pub fn iter_keys(self) -> impl Iterator<Item = u7> {
        iter_u7().filter(move |&i| self.contains(i))
    }
}
impl FromIterator<bool> for KeySet {
    fn from_iter<T: IntoIterator<Item = bool>>(iter: T) -> Self {
        iter.into_iter()
            .positions(|x| x)
            .map(|i| u7::from(i as u8))
            .collect()
    }
}
impl FromIterator<u7> for KeySet {
    fn from_iter<T: IntoIterator<Item = u7>>(iter: T) -> Self {
        let mut key_set = KeySet::new();
        for key in iter {
            key_set.insert(key);
        }
        key_set
    }
}
impl BitOr for KeySet {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        let KeySet([l0, l1]) = self;
        let KeySet([r0, r1]) = rhs;
        KeySet([l0 | r0, l1 | r1])
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct PerKey<T>([T; 128]);
impl<T: Default + Clone> Default for PerKey<T> {
    fn default() -> Self {
        Self::new(&T::default())
    }
}
impl<'a, T> IntoIterator for &'a PerKey<T> {
    type Item = (u7, &'a T);

    type IntoIter = std::iter::Zip<IterU7, std::slice::Iter<'a, T>>;

    fn into_iter(self) -> Self::IntoIter {
        iter_u7().zip(&self.0)
    }
}
impl<'a, T> IntoIterator for &'a mut PerKey<T> {
    type Item = (u7, &'a mut T);

    type IntoIter = std::iter::Zip<IterU7, std::slice::IterMut<'a, T>>;

    fn into_iter(self) -> Self::IntoIter {
        iter_u7().zip(&mut self.0)
    }
}
impl<T> PerKey<T> {
    pub fn new(init: &T) -> Self
    where
        T: Clone,
    {
        Self::from_fn(|_| init.clone())
    }
    pub fn from_fn(mut f: impl FnMut(u7) -> T) -> Self {
        Self(std::array::from_fn(|i| f(u7::from(i as u8))))
    }

    pub fn iter(&self) -> impl Iterator<Item = (u7, &T)> {
        self.into_iter()
    }

    pub fn map<U>(&self, mut f: impl FnMut(u7, &T) -> U) -> PerKey<U>
    where
        T: Clone,
    {
        PerKey::from_fn(|i| f(i, &self[i]))
    }
}
impl<T> Index<u7> for PerKey<T> {
    type Output = T;

    fn index(&self, index: u7) -> &Self::Output {
        &self.0[index.as_int() as usize]
    }
}
impl<T> IndexMut<u7> for PerKey<T> {
    fn index_mut(&mut self, index: u7) -> &mut Self::Output {
        &mut self.0[index.as_int() as usize]
    }
}

pub type IterU7 = std::iter::Map<std::ops::Range<u8>, fn(u8) -> u7>;
pub fn iter_u7() -> IterU7 {
    (0..u7::max_value().as_int()).map(u7::from)
}
