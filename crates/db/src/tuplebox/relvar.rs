use crate::tuplebox::transaction::{Transaction, TupleError};
use crate::tuplebox::RelationId;
use moor_values::util::slice_ref::SliceRef;
use std::collections::HashSet;

/// A reference / handle / pointer to a relation, the actual operations are managed through the
/// transaction.
/// A more convenient handle tied to the lifetime of the transaction.
pub struct RelVar<'a> {
    pub(crate) tx: &'a Transaction,
    pub(crate) id: RelationId,
}

impl<'a> RelVar<'a> {
    /// Seek for a tuple by its indexed domain value.
    pub async fn seek_by_domain(&self, domain: &[u8]) -> Result<SliceRef, TupleError> {
        self.tx.seek_by_domain(self.id, domain).await
    }

    /// Seek for tuples by their indexed codomain value, if there's an index. Panics if there is no
    /// secondary index.
    pub async fn seek_by_codomain(&self, codomain: &[u8]) -> Result<HashSet<Vec<u8>>, TupleError> {
        self.tx.seek_by_codomain(self.id, codomain).await
    }

    /// Insert a tuple into the relation.
    pub async fn insert_tuple(&self, domain: &[u8], codomain: SliceRef) -> Result<(), TupleError> {
        self.tx.insert_tuple(self.id, domain, codomain).await
    }

    /// Update a tuple in the relation.
    pub async fn update_tuple(&self, domain: &[u8], codomain: SliceRef) -> Result<(), TupleError> {
        self.tx.update_tuple(self.id, domain, codomain).await
    }

    /// Upsert a tuple into the relation.
    pub async fn upsert_tuple(&self, domain: &[u8], codomain: SliceRef) -> Result<(), TupleError> {
        self.tx.upsert_tuple(self.id, domain, codomain).await
    }

    /// Remove a tuple from the relation.
    pub async fn remove_by_domain(&self, domain: &[u8]) -> Result<(), TupleError> {
        self.tx.remove_by_domain(self.id, domain).await
    }
}