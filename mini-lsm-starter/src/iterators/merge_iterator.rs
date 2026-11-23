// Copyright (c) 2022-2026 Alex Chi Z
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#![allow(unused_variables)] // TODO(you): remove this lint after implementing this mod
#![allow(dead_code)] // TODO(you): remove this lint after implementing this mod

use std::cmp::{self};
use std::collections::BinaryHeap;
use std::collections::binary_heap::PeekMut;

use anyhow::Result;

use crate::key::KeySlice;

use super::StorageIterator;

// define wrapper around Box<I>
struct HeapWrapper<I: StorageIterator>(pub usize, pub Box<I>);

// custom ordering impl
impl<I: StorageIterator> PartialEq for HeapWrapper<I> {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == cmp::Ordering::Equal
    }
}

impl<I: StorageIterator> Eq for HeapWrapper<I> {}

impl<I: StorageIterator> PartialOrd for HeapWrapper<I> {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<I: StorageIterator> Ord for HeapWrapper<I> {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        self.1
            .key()
            .cmp(&other.1.key())
            .then(self.0.cmp(&other.0))
            .reverse()
    }
}

/// Merge multiple iterators of the same type. If the same key occurs multiple times in some
/// iterators, prefer the one with smaller index.
pub struct MergeIterator<I: StorageIterator> {
    iters: BinaryHeap<HeapWrapper<I>>,
    current: Option<HeapWrapper<I>>,
}

impl<I: StorageIterator> MergeIterator<I> {
    pub fn create(iters: Vec<Box<I>>) -> Self {
        let mut heap: BinaryHeap<HeapWrapper<I>> = iters
            .into_iter()
            .enumerate()
            .filter(
                // filter out invalid iterator
                // and guarantee only valid cursor entries
                |(_, it)| it.is_valid(),
            )
            .map(
                // move iterator wrapper into HeapWrapper
                |(i, it)| HeapWrapper(i, it),
            )
            .collect();

        let current = heap.pop(); // should own the cursor
        Self {
            iters: heap,
            current,
        }
    }
}

impl<I: 'static + for<'a> StorageIterator<KeyType<'a> = KeySlice<'a>>> StorageIterator
    for MergeIterator<I>
{
    type KeyType<'a> = KeySlice<'a>;

    fn key(&self) -> KeySlice<'_> {
        self.current
            .as_ref()
            .expect("key() called on an invalid MergeIterator; callers must check is_valid() first")
            .1
            .key()
    }

    fn value(&self) -> &[u8] {
        self.current
            .as_ref()
            .expect(
                "value() called on an invalid MergeIterator; callers must check is_valid() first",
            )
            .1
            .value()
    }

    fn is_valid(&self) -> bool {
        //// Option is either is_some() or is_none()
        self.current.is_some()
    }

    fn next(&mut self) -> Result<()> {
        // Take ownership of the current cursor.
        // If we return early (error), current stays None.
        let mut cur = self.current.take().expect(
            "next() called on an invalid MergeIterator; callers must check is_valid() first",
        );

        // Skip stale duplicates of cur's key in the heap
        while let Some(mut top) = self.iters.peek_mut() {
            if top.1.key() != cur.1.key() {
                break; // no more duplicates
            }

            if let Err(e) = top.1.next() {
                PeekMut::pop(top); // remove broken cursor from the heap 
                return Err(e);
            }

            if !top.1.is_valid() {
                PeekMut::pop(top); // exhausted current list, remove it from the heap
            }
        }
        cur.1.next()?;

        if cur.1.is_valid() {
            self.iters.push(cur);
        }
        self.current = self.iters.pop();
        Ok(())
    }
}
