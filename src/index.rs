// Copyright 2021 Colin Finck <colin@reactos.org>
// SPDX-License-Identifier: GPL-2.0-or-later

use crate::error::{NtfsError, Result};
use crate::index_entry::{IndexEntryRange, IndexNodeEntryRanges, NtfsIndexEntry};
use crate::indexes::NtfsIndexEntryType;
use crate::structured_values::{NtfsIndexAllocation, NtfsIndexRoot};
use alloc::vec::Vec;
use binread::io::{Read, Seek};
use core::marker::PhantomData;

pub struct NtfsIndex<'n, 'f, E>
where
    E: NtfsIndexEntryType,
{
    index_root: NtfsIndexRoot<'f>,
    index_allocation: Option<NtfsIndexAllocation<'n, 'f>>,
    entry_type: PhantomData<E>,
}

impl<'n, 'f, E> NtfsIndex<'n, 'f, E>
where
    E: NtfsIndexEntryType,
{
    pub fn new(
        index_root: NtfsIndexRoot<'f>,
        index_allocation: Option<NtfsIndexAllocation<'n, 'f>>,
    ) -> Result<Self> {
        if index_root.is_large_index() && index_allocation.is_none() {
            return Err(NtfsError::MissingIndexAllocation {
                position: index_root.position(),
            });
        }

        let entry_type = PhantomData;

        Ok(Self {
            index_root,
            index_allocation,
            entry_type,
        })
    }

    pub fn iter<'i>(&'i self) -> Result<NtfsIndexEntries<'n, 'f, 'i, E>> {
        NtfsIndexEntries::new(self)
    }
}

/// Iterator over
///   all index entries of an index,
///   sorted ascending by the index key,
///   returning an [`NtfsIndexEntry`] for each entry.
///
/// See [`NtfsIndexEntriesAttached`] for an iterator that implements [`Iterator`] and [`FusedIterator`].
pub struct NtfsIndexEntries<'n, 'f, 'i, E>
where
    E: NtfsIndexEntryType,
{
    index: &'i NtfsIndex<'n, 'f, E>,
    inner_iterators: Vec<IndexNodeEntryRanges<E>>,
    following_entries: Vec<IndexEntryRange<E>>,
}

impl<'n, 'f, 'i, E> NtfsIndexEntries<'n, 'f, 'i, E>
where
    E: NtfsIndexEntryType,
{
    fn new(index: &'i NtfsIndex<'n, 'f, E>) -> Result<Self> {
        let inner_iterators = vec![index.index_root.entry_ranges()];
        let following_entries = Vec::new();

        Ok(Self {
            index,
            inner_iterators,
            following_entries,
        })
    }

    pub fn next<'a, T>(&'a mut self, fs: &mut T) -> Option<Result<NtfsIndexEntry<'a, E>>>
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
        let entry_range = loop {
            // Get the iterator from the current node level, if any.
            let iter = self.inner_iterators.last_mut()?;

            // Get the next `IndexEntryRange` from it.
            if let Some(entry_range) = iter.next() {
                // Convert that `IndexEntryRange` to a (lifetime-bound) `NtfsIndexEntry`.
                let entry = entry_range.to_entry(iter.data());

                // Does this entry have a subnode that needs to be iterated first?
                if let Some(subnode_vcn) = entry.subnode_vcn() {
                    // Read the subnode from the filesystem and get an iterator for it.
                    let index_allocation =
                        iter_try!(self.index.index_allocation.as_ref().ok_or_else(|| {
                            NtfsError::MissingIndexAllocation {
                                position: self.index.index_root.position(),
                            }
                        }));
                    let subnode = iter_try!(index_allocation.record_from_vcn(
                        fs,
                        &self.index.index_root,
                        subnode_vcn
                    ));
                    let subnode_iter = subnode.into_entry_ranges();

                    // Save this subnode's iterator and the entry range.
                    // We'll pick up the iterator through `self.inner_iterators.last_mut()` in the
                    // next loop iteration, and we will return that entry as soon as the subnode iterator
                    // has been fully iterated.
                    self.inner_iterators.push(subnode_iter);
                    self.following_entries.push(entry_range);
                } else {
                    // There is no subnode, so our `entry` is next lexicographically.
                    break entry_range;
                }
            } else {
                // The iterator for this subnode level has been fully iterated.
                // Drop it.
                self.inner_iterators.pop();

                // Return the entry, whose subnode we just iterated and which we saved in `following_entries` above.
                // If we just finished iterating the top-level node, `following_entries` is empty and we are done.
                // Otherwise, we can be sure that `inner_iterators` contains the matching iterator for converting
                // `IndexEntryRange` to a (lifetime-bound) `NtfsIndexEntry`.
                let entry_range = self.following_entries.pop()?;
                break entry_range;
            }
        };

        let iter = self.inner_iterators.last().unwrap();
        let entry = entry_range.to_entry(iter.data());
        Some(Ok(entry))
    }
}
