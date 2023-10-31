use strum::{Display, EnumCount, EnumIter};
use uuid::Uuid;

use moor_values::model::objset::ObjSet;
use moor_values::model::WorldStateError;
use moor_values::util::slice_ref::SliceRef;
use moor_values::var::objid::Objid;
use moor_values::AsByteBuffer;

use crate::tuplebox::tuples::TupleError;
use crate::tuplebox::tx::transaction::Transaction;
use crate::tuplebox::RelationId;

/// The set of binary relations that are used to represent the world state in the moor system.
#[repr(usize)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, EnumIter, EnumCount, Display)]
pub enum WorldStateRelation {
    /// Object<->Parent
    ObjectParent = 0,
    /// Object<->Location
    ObjectLocation = 1,
    /// Object->Flags (BitEnum<ObjFlag>)
    ObjectFlags = 2,
    /// Object->Name
    ObjectName = 3,
    /// Object->Owner
    ObjectOwner = 4,

    /// Object->Verbs (Verbdefs)
    ObjectVerbs = 5,
    /// Verb UUID->VerbProgram (Binary)
    VerbProgram = 6,

    /// Object->Properties (Propdefs)
    ObjectPropDefs = 7,
    /// Property UUID->PropertyValue (Var)
    ObjectPropertyValue = 8,
}

impl Into<RelationId> for WorldStateRelation {
    fn into(self) -> RelationId {
        RelationId(self as usize)
    }
}

pub fn composite_key_for(o: Objid, u: &Uuid) -> SliceRef {
    let mut key = o.0.to_le_bytes().to_vec();
    key.extend_from_slice(u.as_bytes());
    SliceRef::from_vec(key)
}

#[repr(usize)]
#[derive(Copy, Clone, Debug, Eq, PartialEq, EnumIter, EnumCount)]
pub enum WorldStateSequences {
    MaximumObject = 0,
}

pub async fn upsert_object_value<Codomain: Clone + Eq + PartialEq + AsByteBuffer>(
    tx: &Transaction,
    rel: WorldStateRelation,
    oid: Objid,
    value: Codomain,
) -> Result<(), WorldStateError> {
    let relation = tx.relation(RelationId(rel as usize)).await;
    if let Err(e) = relation
        .upsert_tuple(oid.as_sliceref(), value.as_sliceref())
        .await
    {
        panic!("Unexpected error: {:?}", e)
    }
    Ok(())
}

#[allow(dead_code)]
pub async fn insert_object_value<Codomain: Clone + Eq + PartialEq + AsByteBuffer>(
    tx: &Transaction,
    rel: WorldStateRelation,
    oid: Objid,
    value: Codomain,
) -> Result<(), WorldStateError> {
    let relation = tx.relation(RelationId(rel as usize)).await;
    match relation
        .insert_tuple(oid.as_sliceref(), value.as_sliceref())
        .await
    {
        Ok(_) => Ok(()),
        Err(TupleError::Duplicate) => {
            Err(WorldStateError::DatabaseError("Duplicate key".to_string()))
        }
        Err(e) => panic!("Unexpected error: {:?}", e),
    }
}

pub async fn get_object_value<Codomain: Clone + Eq + PartialEq + AsByteBuffer>(
    tx: &Transaction,
    rel: WorldStateRelation,
    oid: Objid,
) -> Option<Codomain> {
    let relation = tx.relation(RelationId(rel as usize)).await;
    match relation.seek_by_domain(oid.as_sliceref()).await {
        Ok(v) => Some(Codomain::from_sliceref(v.1)),
        Err(TupleError::NotFound) => None,
        Err(e) => panic!("Unexpected error: {:?}", e),
    }
}

pub async fn get_object_by_codomain<Codomain: Clone + Eq + PartialEq + AsByteBuffer>(
    tx: &Transaction,
    rel: WorldStateRelation,
    codomain: Codomain,
) -> ObjSet {
    let relation = tx.relation(RelationId(rel as usize)).await;
    let result = relation.seek_by_codomain(codomain.as_sliceref()).await;
    let objs = result
        .expect("Unable to seek by codomain")
        .into_iter()
        .map(|v| Objid::from_sliceref(v.0));
    ObjSet::from_oid_iter(objs)
}

pub async fn get_composite_value<Codomain: Clone + Eq + PartialEq + AsByteBuffer>(
    tx: &Transaction,
    rel: WorldStateRelation,
    oid: Objid,
    uuid: Uuid,
) -> Option<Codomain> {
    let key_bytes = composite_key_for(oid, &uuid);
    let relation = tx.relation(RelationId(rel as usize)).await;
    match relation.seek_by_domain(key_bytes).await {
        Ok(v) => Some(Codomain::from_sliceref(v.1)),
        Err(TupleError::NotFound) => None,
        Err(e) => panic!("Unexpected error: {:?}", e),
    }
}

#[allow(dead_code)]
async fn insert_composite_value<Codomain: Clone + Eq + PartialEq + AsByteBuffer>(
    tx: &Transaction,
    rel: WorldStateRelation,
    oid: Objid,
    uuid: Uuid,
    value: Codomain,
) -> Result<(), WorldStateError> {
    let key_bytes = composite_key_for(oid, &uuid);
    let relation = tx.relation(RelationId(rel as usize)).await;
    match relation.insert_tuple(key_bytes, value.as_sliceref()).await {
        Ok(_) => Ok(()),
        Err(TupleError::Duplicate) => Err(WorldStateError::ObjectNotFound(oid)),
        Err(e) => panic!("Unexpected error: {:?}", e),
    }
}

#[allow(dead_code)]
async fn delete_if_exists<Codomain: Clone + Eq + PartialEq + AsByteBuffer>(
    tx: &Transaction,
    rel: WorldStateRelation,
    oid: Objid,
) -> Result<(), WorldStateError> {
    let relation = tx.relation(RelationId(rel as usize)).await;
    match relation.remove_by_domain(oid.as_sliceref()).await {
        Ok(_) => Ok(()),
        Err(TupleError::NotFound) => Ok(()),
        Err(e) => panic!("Unexpected error: {:?}", e),
    }
}

pub async fn delete_composite_if_exists<Codomain: Clone + Eq + PartialEq + AsByteBuffer>(
    tx: &Transaction,
    rel: WorldStateRelation,
    oid: Objid,
    uuid: Uuid,
) -> Result<(), WorldStateError> {
    let key_bytes = composite_key_for(oid, &uuid);
    let relation = tx.relation(RelationId(rel as usize)).await;
    match relation.remove_by_domain(key_bytes).await {
        Ok(_) => Ok(()),
        Err(TupleError::NotFound) => Ok(()),
        Err(e) => panic!("Unexpected error: {:?}", e),
    }
}

pub async fn upsert_obj_uuid_value<Codomain: Clone + Eq + PartialEq + AsByteBuffer>(
    tx: &Transaction,
    rel: WorldStateRelation,
    oid: Objid,
    uuid: Uuid,
    value: Codomain,
) -> Result<(), WorldStateError> {
    let key_bytes = composite_key_for(oid, &uuid);
    let relation = tx.relation(RelationId(rel as usize)).await;
    if let Err(e) = relation.upsert_tuple(key_bytes, value.as_sliceref()).await {
        panic!("Unexpected error: {:?}", e)
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use strum::{EnumCount, IntoEnumIterator};

    use moor_values::model::objset::ObjSet;
    use moor_values::var::objid::Objid;
    use WorldStateRelation::ObjectParent;

    use crate::tuplebox::object_relations::{
        get_object_by_codomain, get_object_value, insert_object_value, upsert_object_value,
        WorldStateRelation, WorldStateSequences,
    };
    use crate::tuplebox::tb::{RelationInfo, TupleBox};

    async fn test_db() -> Arc<TupleBox> {
        let mut relations: Vec<RelationInfo> = WorldStateRelation::iter()
            .map(|wsr| {
                RelationInfo {
                    name: wsr.to_string(),
                    domain_type_id: 0, /* tbd */
                    codomain_type_id: 0,
                    secondary_indexed: false,
                }
            })
            .collect();
        relations[ObjectParent as usize].secondary_indexed = true;
        relations[WorldStateRelation::ObjectLocation as usize].secondary_indexed = true;

        let db = TupleBox::new(1 << 24, 4096, None, &relations, WorldStateSequences::COUNT).await;
        db
    }

    /// Test simple relations mapping oid->oid (with secondary index), independent of all other
    /// worldstate voodoo.
    #[tokio::test]
    async fn test_simple_object() {
        let db = test_db().await;
        let tx = db.clone().start_tx();
        insert_object_value(&tx, ObjectParent, Objid(3), Objid(2))
            .await
            .unwrap();
        insert_object_value(&tx, ObjectParent, Objid(2), Objid(1))
            .await
            .unwrap();
        insert_object_value(&tx, ObjectParent, Objid(1), Objid(0))
            .await
            .unwrap();

        assert_eq!(
            get_object_value::<Objid>(&tx, ObjectParent, Objid(3))
                .await
                .unwrap(),
            Objid(2)
        );
        assert_eq!(
            get_object_value::<Objid>(&tx, ObjectParent, Objid(2))
                .await
                .unwrap(),
            Objid(1)
        );
        assert_eq!(
            get_object_value::<Objid>(&tx, ObjectParent, Objid(1))
                .await
                .unwrap(),
            Objid(0)
        );

        assert_eq!(
            get_object_by_codomain(&tx, ObjectParent, Objid(3)).await,
            ObjSet::from(&[])
        );
        assert_eq!(
            get_object_by_codomain(&tx, ObjectParent, Objid(2)).await,
            ObjSet::from(&[Objid(3)])
        );
        assert_eq!(
            get_object_by_codomain(&tx, ObjectParent, Objid(1)).await,
            ObjSet::from(&[Objid(2)])
        );
        assert_eq!(
            get_object_by_codomain(&tx, ObjectParent, Objid(0)).await,
            ObjSet::from(&[Objid(1)])
        );

        // Now commit and re-verify.
        tx.commit().await.unwrap();
        let tx = db.clone().start_tx();

        assert_eq!(
            get_object_value::<Objid>(&tx, ObjectParent, Objid(3))
                .await
                .unwrap(),
            Objid(2)
        );
        assert_eq!(
            get_object_value::<Objid>(&tx, ObjectParent, Objid(2))
                .await
                .unwrap(),
            Objid(1)
        );
        assert_eq!(
            get_object_value::<Objid>(&tx, ObjectParent, Objid(1))
                .await
                .unwrap(),
            Objid(0)
        );

        assert_eq!(
            get_object_by_codomain(&tx, ObjectParent, Objid(3)).await,
            ObjSet::from(&[])
        );
        assert_eq!(
            get_object_by_codomain(&tx, ObjectParent, Objid(2)).await,
            ObjSet::from(&[Objid(3)])
        );
        assert_eq!(
            get_object_by_codomain(&tx, ObjectParent, Objid(1)).await,
            ObjSet::from(&[Objid(2)])
        );
        assert_eq!(
            get_object_by_codomain(&tx, ObjectParent, Objid(0)).await,
            ObjSet::from(&[Objid(1)])
        );

        // And then update a value and verify.
        upsert_object_value(&tx, ObjectParent, Objid(1), Objid(2))
            .await
            .unwrap();
        assert_eq!(
            get_object_value::<Objid>(&tx, ObjectParent, Objid(1))
                .await
                .unwrap(),
            Objid(2)
        );
        // Verify that the secondary index is updated... First check for new value.
        let children = get_object_by_codomain(&tx, ObjectParent, Objid(2)).await;
        assert_eq!(children.len(), 2);
        assert!(
            children.contains(Objid(1)),
            "Expected children of 2 to contain 1"
        );
        assert!(
            !children.contains(Objid(0)),
            "Expected children of 2 to not contain 0"
        );
        // Now check the old value.
        let children = get_object_by_codomain(&tx, ObjectParent, Objid(0)).await;
        assert_eq!(children, ObjSet::from(&[]));
    }
}
