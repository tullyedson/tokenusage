use super::*;

fn pool() -> ModelPool {
    ModelPool {
        name: "example".into(),
        mode: RouteMode::LoadDistribution,
        members: ["one", "two", "three"]
            .into_iter()
            .map(|id| PoolMember {
                account_id: id.into(),
                model: "small".into(),
            })
            .collect(),
    }
}
#[test]
fn sequential_callers_spread_and_each_returns_to_its_server() {
    let state = Arc::new(LoadDistribution::default());
    let pool = pool();
    for index in 0..9 {
        let assignment = state
            .reserve(&pool, Some(&format!("caller{index}")), &[0, 1, 2])
            .unwrap();
        assert_eq!(assignment.index, index % 3);
        assert!(!assignment.sticky);
    }
    for index in 0..9 {
        let assignment = state
            .reserve(&pool, Some(&format!("caller{index}")), &[0, 1, 2])
            .unwrap();
        assert_eq!(assignment.index, index % 3);
        assert!(assignment.sticky);
    }
    assert!(state.lock().account_requests.is_empty());
    assert!(state.lock().caller_requests.is_empty());
}
#[test]
fn concurrent_reservations_keep_a_caller_together_but_spread_other_callers() {
    let state = Arc::new(LoadDistribution::default());
    let pool = pool();
    let first = state.reserve(&pool, Some("same"), &[0, 1, 2]).unwrap();
    let second = state.reserve(&pool, Some("same"), &[0, 1, 2]).unwrap();
    assert_eq!((first.index, second.index), (0, 0));
    let other = state.reserve(&pool, Some("other"), &[0, 1, 2]).unwrap();
    let third = state.reserve(&pool, Some("third"), &[0, 1, 2]).unwrap();
    assert_eq!((other.index, third.index), (1, 2));
    drop((first, second, other, third));
    assert!(state.lock().account_requests.is_empty());
    assert!(state.lock().caller_requests.is_empty());
}
#[test]
fn active_requests_outweigh_idle_affinity_and_unidentified_requests_rotate() {
    let state = Arc::new(LoadDistribution::default());
    let pool = pool();
    for index in 0..6 {
        let assignment = state.reserve(&pool, None, &[0, 1, 2]).unwrap();
        assert_eq!(assignment.index, index % 3);
    }
    assert!(state.lock().callers.is_empty());
    let busy = state.reserve(&pool, None, &[0]).unwrap();
    let next = state.reserve(&pool, Some("new"), &[0, 1, 2]).unwrap();
    assert_eq!(next.index, 1);
    drop((busy, next));
}
#[test]
fn unavailable_sticky_server_reassigns_and_recovery_does_not_break_stickiness() {
    let state = Arc::new(LoadDistribution::default());
    let pool = pool();
    drop(state.reserve(&pool, Some("caller"), &[0, 1, 2]));
    let replacement = state.reserve(&pool, Some("caller"), &[1, 2]).unwrap();
    assert_eq!(replacement.index, 1);
    drop(replacement);
    let recovered = state.reserve(&pool, Some("caller"), &[0, 1, 2]).unwrap();
    assert_eq!(recovered.index, 1);
    assert!(recovered.sticky);
    let fresh = state.reserve(&pool, Some("fresh"), &[0, 1, 2]).unwrap();
    assert_ne!(fresh.index, 1);
}
#[test]
fn failover_ignores_load_and_affinity_but_reserves_shared_account_capacity() {
    let state = Arc::new(LoadDistribution::default());
    let mut fallback = pool();
    fallback.mode = RouteMode::Failover;
    let first = state
        .reserve(&fallback, Some("caller"), &[0, 1, 2])
        .unwrap();
    let again = state
        .reserve(&fallback, Some("caller"), &[0, 1, 2])
        .unwrap();
    assert_eq!((first.index, again.index), (0, 0));
    assert!(state.lock().callers.is_empty());
    let balanced = state.reserve(&pool(), Some("other"), &[0, 1, 2]).unwrap();
    assert_eq!(balanced.index, 1);
}
#[test]
fn affinity_is_bounded_expires_after_idle_and_active_callers_are_preserved() {
    let state = Arc::new(LoadDistribution::default());
    let pool = pool();
    let active = state.reserve(&pool, Some("active"), &[0, 1, 2]).unwrap();
    for index in 0..AFFINITY_LIMIT + 4 {
        drop(state.reserve(&pool, Some(&format!("idle{index}")), &[0, 1, 2]));
    }
    assert_eq!(state.lock().callers.len(), AFFINITY_LIMIT);
    assert!(state
        .lock()
        .callers
        .contains_key(&(pool.name.clone(), "active".into())));
    let future = Instant::now() + AFFINITY_IDLE + Duration::from_secs(1);
    let new = state
        .reserve_at(&pool, Some("new"), &[0, 1, 2], future)
        .unwrap();
    assert_eq!(state.lock().callers.len(), 2);
    drop((active, new));
}
#[test]
fn configuration_ownership_prevents_late_cleanup_from_changing_new_load() {
    let old = Arc::new(LoadDistribution::default());
    let new = Arc::new(LoadDistribution::default());
    let pool = pool();
    let prior = old.reserve(&pool, Some("caller"), &[0, 1, 2]).unwrap();
    let current = new.reserve(&pool, Some("caller"), &[0, 1, 2]).unwrap();
    drop(prior);
    assert_eq!(new.lock().account_requests.get("one"), Some(&1));
    drop(current);
    assert!(new.lock().account_requests.is_empty());
}
