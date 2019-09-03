/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use crate::clients;
use crate::error::*;
use crate::msg_types::{ServiceStatus, SyncParams, SyncResult};
use logins::PasswordEngine;
use places::{bookmark_sync::store::BookmarksStore, history_sync::store::HistoryStore, PlacesApi};
use std::collections::HashMap;
use std::result;
use std::sync::Mutex;
use std::sync::{atomic::AtomicUsize, Arc, Weak};
use sync15::MemoryCachedState;

const LOGINS_ENGINE: &str = "passwords";
const HISTORY_ENGINE: &str = "history";
const BOOKMARKS_ENGINE: &str = "bookmarks";

pub struct SyncManager {
    mem_cached_state: Option<MemoryCachedState>,
    places: Weak<PlacesApi>,
    logins: Weak<Mutex<PasswordEngine>>,
}

impl SyncManager {
    pub fn new() -> Self {
        Self {
            mem_cached_state: None,
            places: Weak::new(),
            logins: Weak::new(),
        }
    }

    pub fn set_places(&mut self, places: Arc<PlacesApi>) {
        self.places = Arc::downgrade(&places);
    }

    pub fn set_logins(&mut self, logins: Arc<Mutex<PasswordEngine>>) {
        self.logins = Arc::downgrade(&logins);
    }

    pub fn wipe(&mut self, engine: &str) -> Result<()> {
        match engine {
            "logins" => {
                if let Some(logins) = self
                    .logins
                    .upgrade()
                    .as_ref()
                    .map(|l| l.lock().expect("poisoned logins mutex"))
                {
                    logins.wipe()?;
                    Ok(())
                } else {
                    Err(ErrorKind::ConnectionClosed(engine.into()).into())
                }
            }
            "bookmarks" => {
                if let Some(places) = self.places.upgrade() {
                    places.wipe_bookmarks()?;
                    Ok(())
                } else {
                    Err(ErrorKind::ConnectionClosed(engine.into()).into())
                }
            }
            _ => Err(ErrorKind::UnknownEngine(engine.into()).into()),
        }
    }

    pub fn wipe_all(&mut self) -> Result<()> {
        if let Some(logins) = self
            .logins
            .upgrade()
            .as_ref()
            .map(|l| l.lock().expect("poisoned logins mutex"))
        {
            logins.wipe()?;
        }
        if let Some(places) = self.places.upgrade() {
            places.wipe_bookmarks()?;
        }
        Ok(())
    }

    pub fn reset(&mut self, engine: &str) -> Result<()> {
        match engine {
            "logins" => {
                if let Some(logins) = self
                    .logins
                    .upgrade()
                    .as_ref()
                    .map(|l| l.lock().expect("poisoned logins mutex"))
                {
                    logins.reset()?;
                    Ok(())
                } else {
                    Err(ErrorKind::ConnectionClosed(engine.into()).into())
                }
            }
            "bookmarks" | "history" => {
                if let Some(places) = self.places.upgrade() {
                    if engine == "bookmarks" {
                        places.reset_bookmarks()?;
                    } else {
                        places.reset_history()?;
                    }
                    Ok(())
                } else {
                    Err(ErrorKind::ConnectionClosed(engine.into()).into())
                }
            }
            _ => Err(ErrorKind::UnknownEngine(engine.into()).into()),
        }
    }

    pub fn reset_all(&mut self) -> Result<()> {
        if let Some(logins) = self
            .logins
            .upgrade()
            .as_ref()
            .map(|l| l.lock().expect("poisoned logins mutex"))
        {
            logins.reset()?;
        }
        if let Some(places) = self.places.upgrade() {
            places.reset_bookmarks()?;
            places.reset_history()?;
        }
        Ok(())
    }

    pub fn disconnect(&mut self) {
        if let Some(logins) = self
            .logins
            .upgrade()
            .as_ref()
            .map(|l| l.lock().expect("poisoned logins mutex"))
        {
            if let Err(e) = logins.reset() {
                log::error!("Failed to reset logins: {}", e);
            }
        } else {
            log::warn!("Unable to wipe logins, be sure to call set_logins before disconnect if this is surprising");
        }

        if let Some(places) = self.places.upgrade() {
            if let Err(e) = places.reset_bookmarks() {
                log::error!("Failed to reset bookmarks: {}", e);
            }
            if let Err(e) = places.reset_history() {
                log::error!("Failed to reset history: {}", e);
            }
        } else {
            log::warn!("Unable to wipe places, be sure to call set_places before disconnect if this is surprising");
        }
    }

    pub fn sync(&mut self, mut params: SyncParams) -> Result<SyncResult> {
        let mut places = self.places.upgrade();
        let logins = self.logins.upgrade();
        let mut have_engines = vec![];
        if places.is_some() {
            have_engines.push(HISTORY_ENGINE);
            have_engines.push(BOOKMARKS_ENGINE);
        }
        if logins.is_some() {
            have_engines.push(LOGINS_ENGINE);
        }
        check_engine_list(&params.engines_to_sync, &have_engines)?;

        let key_bundle = sync15::KeyBundle::from_ksync_base64(&params.acct_sync_key)?;
        let tokenserver_url = url::Url::parse(&params.acct_tokenserver_url)?;

        let logins_sync = should_sync(&params, LOGINS_ENGINE);
        let bookmarks_sync = should_sync(&params, BOOKMARKS_ENGINE);
        let history_sync = should_sync(&params, HISTORY_ENGINE);

        let places_conn = if bookmarks_sync || history_sync {
            places
                .as_mut()
                .expect("already checked")
                .open_sync_connection()
                .ok()
        } else {
            None
        };
        let l = logins.as_ref().map(|l| l.lock().expect("poisoned mutex"));
        // XXX this isn't ideal, we should have real support for interruption.
        let p = Arc::new(AtomicUsize::new(0));
        let interruptee = sql_support::SqlInterruptScope::new(p);

        let mut mem_cached_state = self.mem_cached_state.take().unwrap_or_default();
        let mut disk_cached_state = params.persisted_state.take();
        // `sync_multiple` takes a &[&dyn Store], but we need something to hold
        // ownership of our stores.
        let mut stores: Vec<Box<dyn sync15::Store>> = vec![];

        if let Some(pc) = places_conn.as_ref() {
            if history_sync {
                stores.push(Box::new(HistoryStore::new(pc, &interruptee)))
            }
            if bookmarks_sync {
                stores.push(Box::new(BookmarksStore::new(pc, &interruptee)))
            }
        }

        if let Some(le) = l.as_ref() {
            assert!(logins_sync, "Should have already checked");
            stores.push(Box::new(logins::LoginStore::new(&le.db)));
        }

        let store_refs: Vec<&dyn sync15::Store> = stores.iter().map(|s| &**s).collect();

        let client_init = sync15::Sync15StorageClientInit {
            key_id: params.acct_key_id.clone(),
            access_token: params.acct_access_token.clone(),
            tokenserver_url,
        };

        let result = sync15::sync_multiple_with_params(
            sync15::SyncMultipleParams {
                stores: &store_refs,
                storage_init: &client_init,
                root_sync_key: &key_bundle,
                interruptee: &interruptee,
                engines_to_state_change: Some(&params.engines_to_change_state),
            },
            &mut disk_cached_state,
            &mut mem_cached_state,
            sync_all_stores_with_clients,
        );

        let status = ServiceStatus::from(result.service_status) as i32;
        let results: HashMap<String, String> = result
            .engine_results
            .into_iter()
            .filter_map(|(e, r)| r.err().map(|r| (e, r.to_string())))
            .collect();

        // Unwrap here can never fail -- it indicates trying to serialize an
        // unserializable type.
        let telemetry_json = serde_json::to_string(&result.telemetry).unwrap();

        Ok(SyncResult {
            status,
            results,
            // XXX FIXME/FINISH ME
            have_declined: false,
            declined: vec![],
            next_sync_allowed_at: None,
            persisted_state: disk_cached_state.unwrap_or_default(),
            telemetry_json: Some(telemetry_json),
        })
    }
}

impl From<sync15::ServiceStatus> for ServiceStatus {
    fn from(s15s: sync15::ServiceStatus) -> Self {
        use sync15::ServiceStatus::*;
        match s15s {
            Ok => ServiceStatus::Ok,
            NetworkError => ServiceStatus::NetworkError,
            ServiceError => ServiceStatus::ServiceError,
            AuthenticationError => ServiceStatus::AuthError,
            BackedOff => ServiceStatus::BackedOff,
            Interrupted => ServiceStatus::OtherError, // Eh...
            OtherError => ServiceStatus::OtherError,
        }
    }
}

fn should_sync(p: &SyncParams, engine: &str) -> bool {
    p.sync_all_engines || p.engines_to_sync.iter().any(|e| e == engine)
}

fn check_engine_list(list: &[String], have_engines: &[&str]) -> Result<()> {
    for e in list {
        if e == "bookmarks" || e == "history" || e == "passwords" {
            if !have_engines.iter().any(|engine| e == engine) {
                return Err(ErrorKind::UnsupportedFeature(e.to_string()).into());
            }
        } else {
            return Err(ErrorKind::UnknownEngine(e.to_string()).into());
        }
    }
    Ok(())
}

fn sync_all_stores_with_clients(
    client: &sync15::Sync15StorageClient,
    global_state: &sync15::GlobalState,
    params: sync15::SyncMultipleParams<'_>,
    sync_result: &mut sync15::SyncResult,
) -> result::Result<sync15::UpdatePersistedGlobalState, failure::Error> {
    let clients_engine = clients::Engine {
        interruptee: params.interruptee,
        client,
        global_state,
        root_sync_key: params.root_sync_key,
        fully_atomic: true,
        settings: clients::Settings {
            fxa_device_id: String::new(),
            name: String::new(),
            client_type: clients::Type::Desktop,
        },
    };
    // Note that a clients engine failing to sync is fatal.
    clients_engine.sync()?;
    // Now sync the remaining stores.
    sync15::sync_all_stores(client, global_state, params, sync_result)
}
