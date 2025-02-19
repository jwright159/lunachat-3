use std::marker::PhantomData;

use async_trait::async_trait;
use axum::extract::FromRef;
use bincode::Options as _;
use key::HighestKeys;
use post::Posts;
use sanitizer::Sanitizer;
use serde::{Deserialize, Serialize};
use sled::{IVec, Tree};
use user::Users;

use crate::error::Result;
use crate::{ok_some, some_ok};

pub mod key;
pub mod post;
pub mod sanitizer;
pub mod user;

lazy_static::lazy_static! {
    pub static ref BINCODE: bincode::config::WithOtherEndian<
        bincode::DefaultOptions,
        bincode::config::BigEndian,
    > = bincode::options().with_big_endian();
}

#[derive(Clone)]
pub struct AppState {
    pub posts: Posts,
    pub users: Users,
    pub highest_keys: HighestKeys,
    pub sanitizer: Sanitizer,
}

impl FromRef<AppState> for Posts {
    fn from_ref(app_state: &AppState) -> Posts {
        app_state.posts.clone()
    }
}

impl FromRef<AppState> for Users {
    fn from_ref(app_state: &AppState) -> Users {
        app_state.users.clone()
    }
}

impl FromRef<AppState> for HighestKeys {
    fn from_ref(app_state: &AppState) -> HighestKeys {
        app_state.highest_keys.clone()
    }
}

impl FromRef<AppState> for Sanitizer {
    fn from_ref(app_state: &AppState) -> Sanitizer {
        app_state.sanitizer.clone()
    }
}

#[async_trait]
pub trait DbTreeLookup<Key, Value>
where
    Key: for<'a> Deserialize<'a> + Serialize,
    Value: for<'a> Deserialize<'a> + Serialize,
{
    fn tree(&self) -> &Tree;

    fn get(&self, key: Key) -> Result<Option<Value>> {
        let key: IVec = BINCODE.serialize(&key)?.into();
        let item = ok_some!(self.tree().get(key));
        Ok(Some(BINCODE.deserialize(&item)?))
    }

    fn iter(&self) -> DbTreeIter<Key, Value> {
        DbTreeIter(self.tree().iter(), PhantomData, PhantomData)
    }

    fn watch(&self) -> sled::Subscriber {
        self.tree().watch_prefix([])
    }

    fn insert(&self, key: Key, value: Value) -> Result<()> {
        let key: IVec = BINCODE.serialize(&key)?.into();
        self.tree().insert(key, BINCODE.serialize(&value)?)?;
        Ok(())
    }

    async fn flush(&self) -> Result<()> {
        self.tree().flush_async().await?;
        Ok(())
    }
}

#[derive(Clone)]
pub struct DbTree<Key, Value>(Tree, PhantomData<Key>, PhantomData<Value>);

impl<Key, Value> DbTree<Key, Value> {
    pub fn new(tree: Tree) -> Self {
        Self(tree, PhantomData, PhantomData)
    }
}

impl<Key, Value> DbTreeLookup<Key, Value> for DbTree<Key, Value>
where
    Key: for<'a> Deserialize<'a> + Serialize,
    Value: for<'a> Deserialize<'a> + Serialize,
{
    fn tree(&self) -> &Tree {
        &self.0
    }
}

pub struct DbTreeIter<Key, Value>(sled::Iter, PhantomData<Key>, PhantomData<Value>);

impl<Key, Value> DbTreeIter<Key, Value>
where
    Key: for<'a> Deserialize<'a>,
    Value: for<'a> Deserialize<'a>,
{
    pub fn keys(self) -> impl DoubleEndedIterator<Item = Result<Key>> {
        self.map(|r| r.map(|(k, _v)| k))
    }

    pub fn values(self) -> impl DoubleEndedIterator<Item = Result<Value>> {
        self.map(|r| r.map(|(_k, v)| v))
    }
}

impl<Key, Value> Iterator for DbTreeIter<Key, Value>
where
    Key: for<'a> Deserialize<'a>,
    Value: for<'a> Deserialize<'a>,
{
    type Item = Result<(Key, Value)>;

    fn next(&mut self) -> Option<Self::Item> {
        let (key, value) = some_ok!(self.0.next());
        Some(Ok((
            BINCODE.deserialize(&key).unwrap(),
            BINCODE.deserialize(&value).unwrap(),
        )))
    }
}

impl<Key, Value> DoubleEndedIterator for DbTreeIter<Key, Value>
where
    Key: for<'a> Deserialize<'a>,
    Value: for<'a> Deserialize<'a>,
{
    fn next_back(&mut self) -> Option<Self::Item> {
        let (key, value) = some_ok!(self.0.next_back());
        Some(Ok((
            BINCODE.deserialize(&key).unwrap(),
            BINCODE.deserialize(&value).unwrap(),
        )))
    }
}

pub type Versions = DbTree<TableType, u64>;

#[derive(Clone, Copy, Serialize, Deserialize)]
#[repr(u64)]
pub enum TableType {
    Posts = 0,
    Users = 1,
    HighestKeys = 2,
}
