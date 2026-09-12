pub mod interval;
pub mod precision;
pub mod shutdown;

/// Locks a mutex and recovers the data when a previous holder panicked. The
/// guarded state in this application is always left consistent between
/// statements, so continuing is preferable to silently killing discovery or
/// the database for the rest of the process lifetime.
pub fn lock_unpoisoned<T>(mutex: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| {
        tracing::warn!("recovered a poisoned mutex; a previous holder panicked");
        poisoned.into_inner()
    })
}

pub fn m_to_nm(m: f64) -> f64 {
    m / 1852.0
}

pub fn nm_to_m(nm: f64) -> f64 {
    nm * 1852.0
}

pub fn m_to_ft(m: f64) -> f64 {
    m * 3.28084
}

pub fn ft_to_nm(ft: f64) -> f64 {
    ft / 6076.118
}

pub fn nm_to_ft(nm: f64) -> f64 {
    nm * 6076.118
}
