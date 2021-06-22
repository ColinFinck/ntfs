// Copyright 2021 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: GPL-2.0-or-later

use crate::error::{NtfsError, Result};
use crate::index_entry::{NtfsIndexEntry, NtfsIndexNodeEntries};
use crate::structured_values::{NewNtfsStructuredValue, NtfsIndexAllocation, NtfsIndexRoot};
use alloc::vec::Vec;
use binread::io::{Read, Seek};
use core::marker::PhantomData;

pub struct NtfsIndex<'n, 'a, K>
where
    K: NewNtfsStructuredValue<'n>,
{
    index_root: &'a NtfsIndexRoot<'n>,
    index_allocation: Option<&'a NtfsIndexAllocation<'n>>,
    key_type: PhantomData<K>,
}

impl<'n, 'a, K> NtfsIndex<'n, 'a, K>
where
    K: NewNtfsStructuredValue<'n>,
{
    pub fn new(
        index_root: &'a NtfsIndexRoot<'n>,
        index_allocation: Option<&'a NtfsIndexAllocation<'n>>,
    ) -> Result<Self> {
        if index_root.is_large_index() && index_allocation.is_none() {
            return Err(NtfsError::MissingIndexAllocation {
                position: index_root.position(),
            });
        }

        let key_type = PhantomData;

        Ok(Self {
            index_root,
            index_allocation,
            key_type,
        })
    }

    pub fn iter<T>(&self, fs: &mut T) -> Result<NtfsIndexEntries<'n, 'a, K>>
    where
        K: NewNtfsStructuredValue<'n>,
        T: Read + Seek,
    {
        NtfsIndexEntries::new(fs, self.index_root, self.index_allocation)
    }
}

enum StackEntry<'n, K>
where
    K: NewNtfsStructuredValue<'n>,
{
    EntryToExplore(NtfsIndexEntry<'n, K>),
    EntryToReturn(NtfsIndexEntry<'n, K>),
    Iter(NtfsIndexNodeEntries<'n, K>),
}

/// Iterator over
///   all index entries of an index,
///   sorted ascending by the index key,
///   returning an [`NtfsIndexEntry`] for each entry.
///
/// See [`NtfsIndexEntriesAttached`] for an iterator that implements [`Iterator`] and [`FusedIterator`].
pub struct NtfsIndexEntries<'n, 'a, K>
where
    K: NewNtfsStructuredValue<'n>,
{
    index_root: &'a NtfsIndexRoot<'n>,
    index_allocation: Option<&'a NtfsIndexAllocation<'n>>,
    stack: Vec<StackEntry<'n, K>>,
}

impl<'n, 'a, K> NtfsIndexEntries<'n, 'a, K>
where
    K: NewNtfsStructuredValue<'n>,
{
    fn new<T>(
        fs: &mut T,
        index_root: &'a NtfsIndexRoot<'n>,
        index_allocation: Option<&'a NtfsIndexAllocation<'n>>,
    ) -> Result<Self>
    where
        K: NewNtfsStructuredValue<'n>,
        T: Read + Seek,
    {
        // Start with the entries of the top-most node of the B-tree.
        // This is given by the `NtfsIndexNodeEntries` iterator over the Index Root entries.
        let stack = vec![StackEntry::Iter(index_root.entries(fs)?)];

        Ok(Self {
            index_root,
            index_allocation,
            stack,
        })
    }

    pub fn next<T>(&mut self, fs: &mut T) -> Option<Result<NtfsIndexEntry<'n, K>>>
    where
        T: Read + Seek,
    {
        // NTFS B-tree indexes are composed out of nodes, with multiple entries per node.
        // Each entry may have a reference to a subnode.
        // If that is the case, the subnode comes before the parent node lexicographically.
        //
        // An example for an unbalanced, but otherwise valid and sorted tree:
        //
        //                                   -------------
        // INDEX ROOT NODE:                  | 4 | 5 | 6 |
        //                                   -------------
        //                                     |
        //                                 ---------
        // INDEX ALLOCATION SUBNODE:       | 1 | 3 |
        //                                 ---------
        //                                       |
        //                                     -----
        // INDEX ALLOCATION SUBNODE:           | 2 |
        //                                     -----
        //
        loop {
            match self.stack.pop()? {
                StackEntry::EntryToExplore(entry) => {
                    // We got an `NtfsIndexEntry` from a previous iteration, which we haven't explored yet.
                    // Check if it has a subnode that needs to be returned first. In that case, push us on the
                    // stack to be returned later and push the `NtfsIndexNodeEntries` iterator from the subnode
                    // to iterate it first.
                    // If this entry has no subnode, just return and forget about it.
                    if let Some(subnode_vcn) = entry.subnode_vcn(fs) {
                        let subnode_vcn = iter_try!(subnode_vcn);
                        let index_allocation = iter_try!(self.index_allocation.ok_or_else(|| {
                            NtfsError::MissingIndexAllocation {
                                position: self.index_root.position(),
                            }
                        }));
                        let subnode = iter_try!(index_allocation.record_from_vcn(
                            fs,
                            &self.index_root,
                            subnode_vcn
                        ));
                        let iter = iter_try!(subnode.entries(fs));
                        self.stack.push(StackEntry::EntryToReturn(entry));
                        self.stack.push(StackEntry::Iter(iter));
                    } else {
                        return Some(Ok(entry));
                    }
                }
                StackEntry::EntryToReturn(entry) => {
                    // We got an `NtfsIndexEntry` that we have already explored, hence all elements before it
                    // have already been returned.
                    // Now it's our turn.
                    return Some(Ok(entry));
                }
                StackEntry::Iter(mut iter) => {
                    // We got an `NtfsIndexNodeEntries` iterator over the entries of a node.
                    // Get the next entry from it, and push the updated iterator and the entry back on the stack.
                    // If this iterator yields no more entries, we are done with this node and can just forget about it.
                    if let Some(entry) = iter.next(fs) {
                        let entry = iter_try!(entry);
                        self.stack.push(StackEntry::Iter(iter));
                        self.stack.push(StackEntry::EntryToExplore(entry));
                    }
                }
            }
        }
    }
}
