use super::{EntityHandle, EntityKind, SimulationError};

#[derive(Clone)]
struct Slot {
    generation: u32,
    dense: Option<usize>,
}
#[derive(Clone)]
pub(crate) struct Entry<T> {
    pub handle: EntityHandle,
    pub value: T,
}

/// Dense spawn order, stable compaction and a deterministic LIFO free list.
/// Exhausted generations are retired rather than wrapping to stale handles.
pub(crate) struct Pool<T> {
    kind: EntityKind,
    slots: Vec<Slot>,
    free: Vec<u32>,
    dense: Vec<Entry<T>>,
}
impl<T: Clone> Clone for Pool<T> {
    fn clone(&self) -> Self {
        let mut free = Vec::with_capacity(self.slots.len());
        free.extend_from_slice(&self.free);
        let mut dense = Vec::with_capacity(self.slots.len());
        dense.extend_from_slice(&self.dense);
        Self {
            kind: self.kind,
            slots: self.slots.clone(),
            free,
            dense,
        }
    }
}
impl<T> Pool<T> {
    pub fn write_checkpoint(
        &self,
        out: &mut crate::checkpoint::Writer,
        mut write: impl FnMut(&T, &mut crate::checkpoint::Writer),
    ) {
        out.u32(self.slots.len() as u32);
        for s in &self.slots {
            out.u32(s.generation);
            out.u32(s.dense.map_or(u32::MAX, |i| i as u32));
        }
        out.u32(self.free.len() as u32);
        for &s in &self.free {
            out.u32(s);
        }
        out.u32(self.dense.len() as u32);
        for e in &self.dense {
            out.handle(e.handle);
            write(&e.value, out);
        }
    }
    pub fn read_checkpoint(
        kind: EntityKind,
        capacity: u32,
        input: &mut crate::checkpoint::Reader<'_>,
        mut read: impl FnMut(
            &mut crate::checkpoint::Reader<'_>,
        ) -> Result<T, crate::checkpoint::CheckpointError>,
    ) -> Result<Self, crate::checkpoint::CheckpointError> {
        use crate::checkpoint::CheckpointError as E;
        let count = input.count(capacity as usize, 8)?;
        if count != capacity as usize {
            return Err(E::Data("pool capacity"));
        }
        let mut slots = Vec::with_capacity(count);
        for _ in 0..count {
            let generation = input.u32()?;
            let dense = input.u32()?;
            if generation == 0 || dense != u32::MAX && dense >= capacity {
                return Err(E::Data("slot generation/index"));
            }
            slots.push(Slot {
                generation,
                dense: if dense == u32::MAX {
                    None
                } else {
                    Some(dense as usize)
                },
            });
        }
        let n = input.count(count, 4)?;
        let mut free = Vec::with_capacity(count);
        let mut seen = vec![false; count];
        for _ in 0..n {
            let slot = input.u32()?;
            let i = slot as usize;
            if i >= count || seen[i] || slots[i].dense.is_some() {
                return Err(E::Data("pool free list"));
            }
            seen[i] = true;
            free.push(slot);
        }
        let n = input.count(count, 9)?;
        let mut dense = Vec::with_capacity(count);
        for index in 0..n {
            let handle = input.handle()?;
            let i = handle.slot() as usize;
            if handle.kind() != kind
                || i >= count
                || seen[i]
                || slots[i].generation != handle.generation()
                || slots[i].dense != Some(index)
            {
                return Err(E::Data("pool dense mapping"));
            }
            seen[i] = true;
            dense.push(Entry {
                handle,
                value: read(input)?,
            });
        }
        for (i, s) in slots.iter().enumerate() {
            if !seen[i] && (s.dense.is_some() || s.generation != u32::MAX) {
                return Err(E::Data("pool missing/retired slot"));
            }
        }
        Ok(Self {
            kind,
            slots,
            free,
            dense,
        })
    }
    pub fn new(kind: EntityKind, capacity: u32) -> Self {
        Self {
            kind,
            slots: vec![
                Slot {
                    generation: 1,
                    dense: None
                };
                capacity as usize
            ],
            free: (0..capacity).rev().collect(),
            dense: Vec::with_capacity(capacity as usize),
        }
    }
    pub fn insert(&mut self, value: T) -> Result<EntityHandle, SimulationError> {
        let slot = self.free.pop().ok_or({
            if self.dense.len() == self.slots.len() {
                SimulationError::Capacity
            } else {
                SimulationError::Exhausted
            }
        })?;
        let record = &mut self.slots[slot as usize];
        let handle = EntityHandle {
            kind: self.kind,
            slot,
            generation: record.generation,
        };
        record.dense = Some(self.dense.len());
        self.dense.push(Entry { handle, value });
        Ok(handle)
    }
    fn index(&self, handle: EntityHandle) -> Option<usize> {
        if handle.kind != self.kind {
            return None;
        }
        let slot = self.slots.get(handle.slot as usize)?;
        if slot.generation != handle.generation {
            return None;
        }
        slot.dense
    }
    pub fn get(&self, handle: EntityHandle) -> Option<&T> {
        Some(&self.dense[self.index(handle)?].value)
    }
    pub fn get_mut(&mut self, handle: EntityHandle) -> Option<&mut T> {
        let index = self.index(handle)?;
        Some(&mut self.dense[index].value)
    }
    pub fn entries(&self) -> &[Entry<T>] {
        &self.dense
    }
    pub fn entries_mut(&mut self) -> &mut [Entry<T>] {
        &mut self.dense
    }
    pub fn len(&self) -> usize {
        self.dense.len()
    }
    pub fn available(&self) -> usize {
        self.free.len()
    }
    pub fn remove(&mut self, handle: EntityHandle) -> Result<(), SimulationError> {
        if self.index(handle).is_none() {
            return Err(SimulationError::InvalidHandle);
        }
        self.retain(|entry| entry.handle != handle);
        Ok(())
    }
    pub fn retain(&mut self, mut keep: impl FnMut(&Entry<T>) -> bool) {
        let mut write = 0;
        for read in 0..self.dense.len() {
            let entry = &self.dense[read];
            let slot = &mut self.slots[entry.handle.slot as usize];
            if keep(entry) {
                slot.dense = Some(write);
                if write != read {
                    self.dense.swap(write, read);
                }
                write += 1;
            } else {
                slot.dense = None;
                if let Some(next) = slot.generation.checked_add(1) {
                    slot.generation = next;
                    self.free.push(entry.handle.slot);
                }
            }
        }
        self.dense.truncate(write);
    }
    pub fn hash_layout(&self, hash: &mut super::StateHasher) {
        hash.u8(self.kind as u8);
        hash.u32(self.slots.len() as u32);
        for slot in &self.slots {
            hash.u32(slot.generation);
            hash.u32(slot.dense.map_or(u32::MAX, |index| index as u32));
        }
        hash.u32(self.free.len() as u32);
        for &slot in &self.free {
            hash.u32(slot);
        }
        hash.u32(self.dense.len() as u32);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sparse_removal_preserves_order_and_invalidates_all_removed_handles() {
        let mut pool = Pool::new(EntityKind::Projectile, 7);
        let handles: Vec<_> = (0..7).map(|value| pool.insert(value).unwrap()).collect();
        pool.retain(|entry| entry.value % 2 == 1);
        assert_eq!(
            pool.entries()
                .iter()
                .map(|entry| entry.value)
                .collect::<Vec<_>>(),
            vec![1, 3, 5]
        );
        for (value, &handle) in handles.iter().enumerate() {
            assert_eq!(
                pool.get(handle).copied(),
                if value % 2 == 1 {
                    Some(value as i32)
                } else {
                    None
                }
            );
        }
        let first = pool.insert(10).unwrap();
        let second = pool.insert(11).unwrap();
        assert_eq!((first.slot(), first.generation()), (6, 2));
        assert_eq!((second.slot(), second.generation()), (4, 2));
        assert_eq!(
            pool.entries()
                .iter()
                .map(|entry| entry.value)
                .collect::<Vec<_>>(),
            vec![1, 3, 5, 10, 11]
        );
        for (index, &handle) in handles.iter().enumerate() {
            if index % 2 == 0 {
                assert!(pool.get(handle).is_none());
            }
        }
    }
    #[test]
    fn retired_generation_never_revalidates_old_handle() {
        let mut pool = Pool::new(EntityKind::Enemy, 1);
        pool.slots[0].generation = u32::MAX;
        let handle = pool.insert(42).unwrap();
        pool.remove(handle).unwrap();
        assert_eq!(pool.get(handle), None);
        assert_eq!(pool.insert(43), Err(SimulationError::Exhausted));
    }
    #[test]
    fn checkpoint_keeps_retired_slots_free_order_dense_mapping_and_rejects_duplicates() {
        use crate::checkpoint::{CheckpointError, MAX_CHECKPOINT_BYTES, Reader, Writer};
        let mut pool = Pool::new(EntityKind::Enemy, 4);
        pool.slots[0].generation = u32::MAX;
        let a = pool.insert(1u32).unwrap();
        let b = pool.insert(2u32).unwrap();
        let c = pool.insert(3u32).unwrap();
        pool.remove(a).unwrap();
        pool.remove(c).unwrap();
        let mut w = Writer::new(b"GZPOOL01");
        pool.write_checkpoint(&mut w, |v, w| w.u32(*v));
        let mut bytes = w.finish(MAX_CHECKPOINT_BYTES).unwrap();
        let mut r = Reader::new(&bytes, b"GZPOOL01", MAX_CHECKPOINT_BYTES).unwrap();
        let mut restored =
            Pool::read_checkpoint(EntityKind::Enemy, 4, &mut r, |r| r.u32()).unwrap();
        r.finish().unwrap();
        assert_eq!(restored.get(b), Some(&2));
        assert_eq!(restored.insert(4).unwrap(), pool.insert(4).unwrap());
        assert!(restored.get(a).is_none());
        bytes[12..16].copy_from_slice(&0u32.to_le_bytes());
        let end = bytes.len() - 8;
        let hash = crate::checkpoint::fingerprint(&bytes[..end]);
        bytes[end..].copy_from_slice(&hash.to_le_bytes());
        let mut r = Reader::new(&bytes, b"GZPOOL01", MAX_CHECKPOINT_BYTES).unwrap();
        assert!(matches!(
            Pool::<u32>::read_checkpoint(EntityKind::Enemy, 4, &mut r, |r| r.u32()),
            Err(CheckpointError::Data(_))
        ));
    }
}
