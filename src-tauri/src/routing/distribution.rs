//! Request reservations and caller affinity, scoped to one routing configuration.
//! Nothing here reads prompts, stores credentials, or publishes caller identifiers.
use super::config::{ModelPool, PoolMember, RouteMode};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex, MutexGuard},
    time::{Duration, Instant},
};

const AFFINITY_LIMIT: usize = 4096;
const AFFINITY_IDLE: Duration = Duration::from_secs(30 * 60);
type CallerKey = (String, String);

struct Affinity {
    member: PoolMember,
    touched: Instant,
}
#[derive(Default)]
struct State {
    callers: BTreeMap<CallerKey, Affinity>,
    caller_requests: BTreeMap<CallerKey, usize>,
    account_requests: BTreeMap<String, usize>,
    cursors: BTreeMap<String, usize>,
}
#[derive(Default)]
pub struct LoadDistribution(Mutex<State>);
impl LoadDistribution {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.0.lock().unwrap_or_else(|error| error.into_inner())
    }
    /// Selection and reservation are atomic, including concurrent first requests
    /// from the same caller. Candidates have already passed known cooldown checks.
    pub fn reserve(
        self: &Arc<Self>,
        pool: &ModelPool,
        caller: Option<&str>,
        candidates: &[usize],
    ) -> Option<Assignment> {
        self.reserve_at(pool, caller, candidates, Instant::now())
    }
    fn reserve_at(
        self: &Arc<Self>,
        pool: &ModelPool,
        caller: Option<&str>,
        candidates: &[usize],
        now: Instant,
    ) -> Option<Assignment> {
        let first = *candidates.first()?;
        let mut state = self.lock();
        let State {
            callers,
            caller_requests,
            ..
        } = &mut *state;
        callers.retain(|key, value| {
            caller_requests.contains_key(key) || now.duration_since(value.touched) < AFFINITY_IDLE
        });
        let key = (pool.mode == RouteMode::LoadDistribution)
            .then(|| caller.map(|caller| (pool.name.clone(), caller.to_owned())))
            .flatten();
        let sticky = key
            .as_ref()
            .and_then(|key| state.callers.get(key))
            .and_then(|affinity| {
                candidates
                    .iter()
                    .copied()
                    .find(|index| pool.members[*index] == affinity.member)
            });
        let index = if pool.mode == RouteMode::Failover {
            first
        } else if let Some(index) = sticky {
            index
        } else {
            let cursor = state.cursors.get(&pool.name).copied().unwrap_or(0);
            let mut assigned = BTreeMap::<&str, usize>::new();
            for affinity in state.callers.values() {
                *assigned.entry(&affinity.member.account_id).or_default() += 1;
            }
            let index = candidates
                .iter()
                .copied()
                .min_by_key(|index| {
                    let account = &pool.members[*index].account_id;
                    (
                        state.account_requests.get(account).copied().unwrap_or(0),
                        assigned.get(account.as_str()).copied().unwrap_or(0),
                        (index + pool.members.len() - cursor) % pool.members.len(),
                    )
                })
                .expect("Nonempty candidates");
            state
                .cursors
                .insert(pool.name.clone(), (index + 1) % pool.members.len());
            index
        };
        let member = &pool.members[index];
        if let Some(key) = &key {
            if !state.callers.contains_key(key) && state.callers.len() >= AFFINITY_LIMIT {
                let oldest = state
                    .callers
                    .iter()
                    .filter(|(key, _)| !state.caller_requests.contains_key(*key))
                    .min_by_key(|(_, value)| value.touched)
                    .map(|(key, _)| key.clone());
                if let Some(oldest) = oldest {
                    state.callers.remove(&oldest);
                }
            }
            state.callers.insert(
                key.clone(),
                Affinity {
                    member: member.clone(),
                    touched: now,
                },
            );
            *state.caller_requests.entry(key.clone()).or_default() += 1;
        }
        *state
            .account_requests
            .entry(member.account_id.clone())
            .or_default() += 1;
        Some(Assignment {
            owner: self.clone(),
            key,
            member: member.clone(),
            index,
            sticky: sticky.is_some(),
        })
    }
}

/// Held while queued, during preflight, and until the JSON response or stream ends.
/// Drop is synchronous so an aborted task cannot leak load or require cleanup I/O.
pub struct Assignment {
    owner: Arc<LoadDistribution>,
    key: Option<CallerKey>,
    member: PoolMember,
    pub index: usize,
    pub sticky: bool,
}
impl Drop for Assignment {
    fn drop(&mut self) {
        let mut state = self.owner.lock();
        decrement(&mut state.account_requests, &self.member.account_id);
        if let Some(key) = &self.key {
            decrement(&mut state.caller_requests, key);
            if let Some(affinity) = state.callers.get_mut(key) {
                if affinity.member == self.member {
                    affinity.touched = Instant::now();
                }
            }
        }
    }
}
fn decrement<K: Ord>(counts: &mut BTreeMap<K, usize>, key: &K) {
    if let Some(count) = counts.get_mut(key) {
        *count -= 1;
        if *count == 0 {
            counts.remove(key);
        }
    }
}

#[cfg(test)]
mod tests;
