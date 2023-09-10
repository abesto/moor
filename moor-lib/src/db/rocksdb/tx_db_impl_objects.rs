use std::collections::{HashMap, HashSet};
use std::convert::TryInto;

use moor_value::model::defset::HasUuid;
use moor_value::model::objects::{ObjAttrs, ObjFlag};
use moor_value::model::objset::ObjSet;
use moor_value::{AsByteBuffer, NOTHING};

use moor_value::model::WorldStateError;
use moor_value::util::bitenum::BitEnum;
use moor_value::util::slice_ref::SliceRef;
use moor_value::var::objid::Objid;

use crate::db::rocksdb::tx_db_impl::{
    cf_for, composite_key_for, err_is_objnjf, get_objset, get_oid_or_nothing, get_oid_value,
    oid_key, set_objset, set_oid_value, write_cf, RocksDbTx,
};
use crate::db::rocksdb::ColumnFamilies;

// Methods for manipulation of objects, their owners, flags, contents, parents, etc.
impl<'a> RocksDbTx<'a> {
    #[tracing::instrument(skip(self))]
    pub fn object_valid(&self, o: Objid) -> Result<bool, anyhow::Error> {
        let cf = cf_for(&self.cf_handles, ColumnFamilies::ObjectFlags);
        let ok = oid_key(o);
        let ov = self.tx.get_cf(cf, ok)?;
        Ok(ov.is_some())
    }
    #[tracing::instrument(skip(self))]
    pub fn create_object(
        &self,
        oid: Option<Objid>,
        attrs: ObjAttrs,
    ) -> Result<Objid, anyhow::Error> {
        let oid = match oid {
            None => self.next_object_id()?,
            Some(oid) => {
                // If this object already exists, that's an error.
                if self.object_valid(oid)? {
                    return Err(WorldStateError::ObjectAlreadyExists(oid).into());
                }
                oid
            }
        };

        // None (#-1) owner becomes
        let owner = attrs.owner.unwrap_or(oid);
        set_oid_value(
            cf_for(&self.cf_handles, ColumnFamilies::ObjectOwner),
            &self.tx,
            oid,
            owner,
        )?;

        // Set initial name
        let name = attrs.name.unwrap_or_else(|| format!("Object #{}", oid.0));
        self.set_object_name(oid, name.clone())?;

        // Establish initial `contents` and `children` vectors, initially empty.
        let c_cf = cf_for(&self.cf_handles, ColumnFamilies::ObjectContents);
        set_objset(c_cf, &self.tx, oid, ObjSet::new())?;

        self.update_object_children(oid, ObjSet::new())?;

        if let Some(parent) = attrs.parent {
            self.set_object_parent(oid, parent)?;
        }

        if let Some(location) = attrs.location {
            self.set_object_location(oid, location)?;
        }

        let default_object_flags = BitEnum::new();
        self.set_object_flags(oid, attrs.flags.unwrap_or(default_object_flags))?;

        self.update_highest_object_id(oid)?;
        Ok(oid)
    }
    #[tracing::instrument(skip(self))]
    pub fn recycle_object(&self, obj: Objid) -> Result<(), anyhow::Error> {
        // First go through and move all objects that are in this object's contents to the
        // to #-1.  It's up to the caller here to execute :exitfunc on all of them before invoking
        // this method.
        let contents = self.get_object_contents(obj)?;
        for c in contents.iter() {
            self.set_object_location(c, NOTHING)?;
        }

        // Now reparent all our immediate children to our parent.
        // This should properly move all properties all the way down the chain.
        let parent = self.get_object_parent(obj)?;
        let children = self.get_object_children(obj)?;
        for c in children.iter() {
            self.set_object_parent(c, parent)?;
        }

        // Now we can remove this object from all relevant column families.
        // First the simple ones which are keyed on the object id.
        let oid_cfs = vec![
            ColumnFamilies::ObjectFlags,
            ColumnFamilies::ObjectName,
            ColumnFamilies::ObjectOwner,
            ColumnFamilies::ObjectParent,
            ColumnFamilies::ObjectLocation,
            ColumnFamilies::ObjectContents,
            ColumnFamilies::ObjectChildren,
            ColumnFamilies::ObjectVerbs,
        ];
        let ok = oid_key(obj);
        for cf in oid_cfs {
            let cf = cf_for(&self.cf_handles, cf);
            self.tx.delete_cf(cf, ok)?;
        }
        // Get the propdefs and remove all the property values.
        let propdefs = self.get_propdefs(obj)?;
        for p in propdefs.iter() {
            let vk = composite_key_for(obj, &p);
            self.tx.delete_cf(
                cf_for(&self.cf_handles, ColumnFamilies::ObjectPropertyValue),
                vk,
            )?;
        }

        // Now remove the propdefs themselves.
        self.tx
            .delete_cf(cf_for(&self.cf_handles, ColumnFamilies::ObjectPropDefs), ok)?;

        // That that's that.
        Ok(())
    }

    #[tracing::instrument(skip(self))]
    pub fn set_object_parent(&self, o: Objid, new_parent: Objid) -> Result<(), anyhow::Error> {
        let parent_cf = cf_for(&self.cf_handles, ColumnFamilies::ObjectParent);
        let property_value_cf = cf_for(&self.cf_handles, ColumnFamilies::ObjectPropertyValue);

        // TODO this is all very wasteful for net-new objects, which have no children or properties
        //   to move around.

        // Steps for object re-parenting:

        // Get o's old-parents's children
        //      remove o from it, and save.
        // Walk existing descendant tree of O and find any props that they inherited from old-parent
        // or any of its ancestors up to the most recent common ancestor, remove them.
        // Get o's new-parent's children list add o to it, and save.
        // Walk same descendant tree, and add props defined by new-parent and *its* ancestors, up to
        // shared one.
        // Set o's parent field.

        // This will find a) our shared ancestor, b) all ancestors not shared with new ancestor,
        // c) all the new ancestors we'd have after the reparenting, all in one go. Hopefully.
        // TODO: the argument order seems backward here. I was able to make it work by flipping
        //   new_parent and o, but I need to get to the bottom of this and fix it properly.
        let (_shared_ancestor, new_ancestors, old_ancestors) =
            self.closest_common_ancestor_with_ancestors(new_parent, o)?;

        // Remove from _me_ any of the properties defined by any of my ancestors
        let old_props = self.get_propdefs(o)?;
        let mut delort_props = vec![];
        for p in old_props.iter() {
            if old_ancestors.contains(&p.definer()) {
                delort_props.push(p.uuid());
                let vk = composite_key_for(o, &p);
                self.tx.delete_cf(property_value_cf, vk)?;
            }
        }
        let new_props = old_props.with_all_removed(&delort_props);
        self.update_propdefs(o, new_props)?;

        // Now walk all-my-children and destroy all the properties whose definer is me or any
        // of my ancestors not shared by the new parent.
        let descendants = self.descendants(o)?;

        let mut descendant_props = HashMap::new();
        for c in descendants.iter() {
            let mut inherited_props = vec![];
            // Remove the set values.
            let old_props = self.get_propdefs(c)?;
            for p in old_props.iter() {
                if old_ancestors.contains(&p.definer()) {
                    inherited_props.push(p.uuid());
                    let vk = composite_key_for(c, &p);
                    self.tx.delete_cf(property_value_cf, vk)?;
                }
            }
            // And update the property list to not include them
            let new_props = old_props.with_all_removed(&inherited_props);

            // We're not actually going to *set* these yet because we are going to add, later.
            descendant_props.insert(c, new_props);
        }

        // If this is a new object it won't have a parent, old parent this will come up not-found,
        // and if that's the case we can ignore that.
        match get_oid_value(parent_cf, &self.tx, o) {
            Ok(old_parent) => {
                if old_parent == new_parent {
                    return Ok(());
                }
                if old_parent != NOTHING {
                    // Prune us out of the old parent's children list.
                    let old_children = self.get_object_children(old_parent)?;
                    let new_children =
                        ObjSet::from_oid_iter(old_children.iter().filter(|&x| x != o));
                    self.update_object_children(old_parent, new_children)?;
                }
            }
            Err(e) if !err_is_objnjf(&e) => {
                // Object not found is fine, we just don't have a parent yet.
                return Err(e);
            }
            Err(_) => {}
        };
        set_oid_value(parent_cf, &self.tx, o, new_parent)?;

        if new_parent == NOTHING {
            return Ok(());
        }
        let new_children = self.get_object_children(new_parent)?.with_inserted(o);
        self.update_object_children(new_parent, new_children)?;

        // Now walk all my new descendants and give them the properties that derive from any
        // ancestors they don't already share.

        // Now collect properties defined on the new ancestors.
        let mut new_props = vec![];
        for a in new_ancestors {
            let props = self.get_propdefs(a)?;
            for p in props.iter() {
                if p.definer() == a {
                    new_props.push(p.clone())
                }
            }
        }
        // Then put clear copies on each of the descendants ... and me.
        // This really just means defining the property with no value, which is what we do.
        let descendants = self.descendants(o)?;
        for c in descendants.iter().chain(std::iter::once(o)) {
            // Check if we have a cached/modified copy from above in descendant_props
            let c_props = match descendant_props.remove(&c) {
                None => self.get_propdefs(c)?,
                Some(props) => props,
            };
            let c_props = c_props.with_all_added(&new_props);
            self.update_propdefs(o, c_props)?;
        }
        Ok(())
    }
    #[tracing::instrument(skip(self))]
    pub fn get_object_children(&self, o: Objid) -> Result<ObjSet, anyhow::Error> {
        let cf = self.cf_handles[(ColumnFamilies::ObjectChildren as u8) as usize];
        Ok(get_objset(cf, &self.tx, o).unwrap_or_else(|_| ObjSet::new()))
    }
    #[tracing::instrument(skip(self))]
    pub fn get_object_name(&self, o: Objid) -> Result<String, anyhow::Error> {
        let cf = cf_for(&self.cf_handles, ColumnFamilies::ObjectName);
        let ok = oid_key(o);
        let name_bytes = self.tx.get_cf(cf, ok)?;
        let Some(name_bytes) = name_bytes else {
            return Err(WorldStateError::ObjectNotFound(o).into());
        };
        Ok(String::from_sliceref(SliceRef::from_bytes(&name_bytes)))
    }
    #[tracing::instrument(skip(self))]
    pub fn set_object_name(&self, o: Objid, name: String) -> Result<(), anyhow::Error> {
        let cf = cf_for(&self.cf_handles, ColumnFamilies::ObjectName);
        let ok = oid_key(o);
        write_cf(&self.tx, cf, &ok, &name)?;
        Ok(())
    }
    #[tracing::instrument(skip(self))]
    pub fn get_object_flags(&self, o: Objid) -> Result<BitEnum<ObjFlag>, anyhow::Error> {
        let cf = cf_for(&self.cf_handles, ColumnFamilies::ObjectFlags);
        let ok = oid_key(o);
        let flag_bytes = self.tx.get_cf(cf, ok)?;
        let Some(flag_bytes) = flag_bytes else {
            return Err(WorldStateError::ObjectNotFound(o).into());
        };
        Ok(BitEnum::from_sliceref(SliceRef::from_bytes(&flag_bytes)))
    }
    #[tracing::instrument(skip(self))]
    pub fn set_object_flags(&self, o: Objid, flags: BitEnum<ObjFlag>) -> Result<(), anyhow::Error> {
        let cf = cf_for(&self.cf_handles, ColumnFamilies::ObjectFlags);
        let ok = oid_key(o);
        write_cf(&self.tx, cf, &ok, &flags)?;
        Ok(())
    }
    #[tracing::instrument(skip(self))]
    pub fn get_object_owner(&self, o: Objid) -> Result<Objid, anyhow::Error> {
        let cf = cf_for(&self.cf_handles, ColumnFamilies::ObjectOwner);
        get_oid_or_nothing(cf, &self.tx, o)
    }
    #[tracing::instrument(skip(self))]
    pub fn set_object_owner(&self, o: Objid, owner: Objid) -> Result<(), anyhow::Error> {
        let cf = self.cf_handles[(ColumnFamilies::ObjectOwner as u8) as usize];
        set_oid_value(cf, &self.tx, o, owner)
    }
    #[tracing::instrument(skip(self))]
    pub fn get_object_parent(&self, o: Objid) -> Result<Objid, anyhow::Error> {
        let cf = self.cf_handles[(ColumnFamilies::ObjectParent as u8) as usize];
        get_oid_or_nothing(cf, &self.tx, o)
    }
    #[tracing::instrument(skip(self))]
    pub fn get_object_location(&self, o: Objid) -> Result<Objid, anyhow::Error> {
        let cf = self.cf_handles[(ColumnFamilies::ObjectLocation as u8) as usize];
        get_oid_or_nothing(cf, &self.tx, o)
    }
    #[tracing::instrument(skip(self))]
    pub fn get_object_contents(&self, o: Objid) -> Result<ObjSet, anyhow::Error> {
        let cf = self.cf_handles[(ColumnFamilies::ObjectContents as u8) as usize];
        get_objset(cf, &self.tx, o)
    }
    #[tracing::instrument(skip(self))]
    pub fn set_object_location(
        &self,
        what: Objid,
        new_location: Objid,
    ) -> Result<(), anyhow::Error> {
        // Detect recursive move
        let mut oid = new_location;
        loop {
            if oid == NOTHING {
                break;
            }
            if oid == what {
                return Err(WorldStateError::RecursiveMove(what, new_location).into());
            }
            oid = self.get_object_location(oid).unwrap_or(NOTHING);
        }

        // Get o's location, get its contents, remove o from old contents, put contents back
        // without it. Set new location, get its contents, add o to contents, put contents
        // back with it. Then update the location of o.

        let l_cf = cf_for(&self.cf_handles, ColumnFamilies::ObjectLocation);
        let c_cf = cf_for(&self.cf_handles, ColumnFamilies::ObjectContents);

        // Get and remove from contents of old location, if we had any.
        match get_oid_or_nothing(l_cf, &self.tx, what) {
            Ok(NOTHING) => {
                // Object not found is fine, we just don't have a location yet.
            }
            Ok(old_location) => {
                if old_location == new_location {
                    return Ok(());
                }
                if old_location != NOTHING {
                    let c_cf = cf_for(&self.cf_handles, ColumnFamilies::ObjectContents);
                    let old_contents = get_objset(c_cf, &self.tx, old_location)?;
                    let old_contents = old_contents.with_removed(what);
                    set_objset(c_cf, &self.tx, old_location, old_contents)?;
                }
            }
            Err(e) => {
                return Err(e);
            }
        }
        // Set new location.
        set_oid_value(l_cf, &self.tx, what, new_location)?;

        if new_location == NOTHING {
            return Ok(());
        }

        // Get and add to contents of new location.
        let new_contents = get_objset(c_cf, &self.tx, new_location)
            .unwrap_or_else(|_| ObjSet::new())
            .with_inserted(what);
        set_objset(c_cf, &self.tx, new_location, new_contents)?;
        Ok(())
    }
}

// Private helper methods related to objects.
impl<'a> RocksDbTx<'a> {
    // TODO sucks to do this transactionally, but we need to make sure we don't create a duplicate
    // we could do this an atomic increment on the whole DB, but in the long run we actually want to
    // get rid of object ids entirely.
    // (One thought is to simply make Objid u128 and use UUIDs for object ids and then just handle
    // any totally-theoretical collisions optimistically by relying on commit-time conflicts to
    // suss them out. There's some code in MOO cores that *implies* the concept of monotonically
    // increment OIds, but it is not necessary, I'm pretty sure)
    fn next_object_id(&self) -> Result<Objid, anyhow::Error> {
        let cf = self.cf_handles[(ColumnFamilies::ObjectIds as u8) as usize];
        let key = "OBJECT_ID_COUNTER".as_bytes();
        let id_bytes = self.tx.get_cf(cf, key)?;
        let id = match id_bytes {
            None => {
                let id = Objid(0);
                let id_bytes = id.0.to_be_bytes().to_vec();
                self.tx.put_cf(cf, key, id_bytes)?;
                id
            }
            Some(id_bytes) => {
                let id_bytes = id_bytes.as_slice();
                let id_bytes: [u8; 8] = id_bytes.try_into().unwrap();
                let id = Objid(i64::from_be_bytes(id_bytes) + 1);
                let id_bytes = id.0.to_be_bytes().to_vec();
                self.tx.put_cf(cf, key, id_bytes)?;
                id
            }
        };
        Ok(id)
    }

    /// Update the highest object ID if the given ID is higher than the current highest.
    fn update_highest_object_id(&self, oid: Objid) -> Result<(), anyhow::Error> {
        let cf = self.cf_handles[(ColumnFamilies::ObjectIds as u8) as usize];
        let key = "OBJECT_ID_COUNTER".as_bytes();
        let id_bytes = self.tx.get_cf(cf, key)?;
        match id_bytes {
            None => {
                let id_bytes = oid.0.to_be_bytes().to_vec();
                self.tx.put_cf(cf, key, id_bytes)?;
            }
            Some(id_bytes) => {
                let id_bytes = id_bytes.as_slice();
                let id_bytes: [u8; 8] = id_bytes.try_into().unwrap();
                let id = Objid(i64::from_be_bytes(id_bytes));
                if oid > id {
                    let id_bytes = oid.0.to_be_bytes().to_vec();
                    self.tx.put_cf(cf, key, id_bytes)?;
                }
            }
        };
        Ok(())
    }

    fn closest_common_ancestor_with_ancestors(
        &self,
        a: Objid,
        b: Objid,
    ) -> Result<(Option<Objid>, HashSet<Objid>, HashSet<Objid>), anyhow::Error> {
        let mut ancestors_a = HashSet::new();
        let mut search_a = a;

        let mut ancestors_b = HashSet::new();
        let mut search_b = b;

        loop {
            if search_a == NOTHING && search_b == NOTHING {
                return Ok((None, ancestors_a, ancestors_b)); // No common ancestor found
            }

            if ancestors_b.contains(&search_a) {
                return Ok((Some(search_a), ancestors_a, ancestors_b)); // Common ancestor found
            }

            if ancestors_a.contains(&search_b) {
                return Ok((Some(search_b), ancestors_a, ancestors_b)); // Common ancestor found
            }

            if search_a != NOTHING {
                ancestors_a.insert(search_a);
                let parent_cf = self.cf_handles[((ColumnFamilies::ObjectParent) as u8) as usize];
                let parent = get_oid_or_nothing(parent_cf, &self.tx, search_a)?;
                search_a = parent;
            }

            if search_b != NOTHING {
                ancestors_b.insert(search_b);
                let parent_cf = self.cf_handles[((ColumnFamilies::ObjectParent) as u8) as usize];
                let parent = get_oid_or_nothing(parent_cf, &self.tx, search_b)?;
                search_b = parent;
            }
        }
    }

    pub(crate) fn descendants(&self, obj: Objid) -> Result<ObjSet, anyhow::Error> {
        let mut search_queue = vec![obj];

        let all_children = std::iter::from_fn(move || {
            while let Some(search_obj) = search_queue.pop() {
                match self.get_object_children(search_obj) {
                    Ok(new_children) => {
                        // Add new children to the search queue
                        search_queue.extend(new_children.iter());

                        // Extend the iterator with new children
                        return Some(new_children.iter());
                    }
                    Err(_) => continue,
                }
            }
            None
        })
        .flatten();

        Ok(ObjSet::from_oid_iter(all_children))
    }

    fn update_object_children(
        &self,
        obj: Objid,
        new_cildren: ObjSet,
    ) -> Result<(), WorldStateError> {
        let children_cf = self.cf_handles[((ColumnFamilies::ObjectChildren) as u8) as usize];
        set_objset(children_cf, &self.tx, obj, new_cildren)
    }
}
