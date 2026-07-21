//! Feature-gated Prometheus metrics for the MCP adapter bridge.
//!
//! When the `metrics` cargo feature is enabled, each helper records to a
//! lazily-registered Prometheus counter / gauge / histogram. When
//! disabled (the default), every helper compiles to an empty function
//! body so the crate carries zero prometheus dependency.

#[cfg(feature = "metrics")]
mod inner {
    use prometheus::{
        Histogram, IntCounter, IntGauge, exponential_buckets, register_histogram,
        register_int_counter, register_int_gauge,
    };
    use std::sync::LazyLock;

    static MCP_CALL_DURATION_SECONDS: LazyLock<Histogram> = LazyLock::new(|| {
        register_histogram!(
            "computer_hub_mcp_call_duration_seconds",
            "MCP server response latency for tool calls.",
            exponential_buckets(0.01, 2.0, 14).expect("valid bucket params")
        )
        .expect("computer_hub_mcp_call_duration_seconds must register once")
    });

    static MCP_ERRORS_TOTAL: LazyLock<IntCounter> = LazyLock::new(|| {
        register_int_counter!(
            "computer_hub_mcp_errors_total",
            "Errors in the MCP adapter pipeline (transport, protocol, or serialization)."
        )
        .expect("computer_hub_mcp_errors_total must register once")
    });

    static MCP_CALL_TIMEOUTS_TOTAL: LazyLock<IntCounter> = LazyLock::new(|| {
        register_int_counter!(
            "computer_hub_mcp_call_timeouts_total",
            "MCP tool calls that hit the adapter backstop timeout."
        )
        .expect("computer_hub_mcp_call_timeouts_total must register once")
    });

    static MCP_RETRIES_ATTEMPTED_TOTAL: LazyLock<IntCounter> = LazyLock::new(|| {
        register_int_counter!(
            "computer_hub_mcp_retries_attempted_total",
            "MCP call retries attempted by the adapter."
        )
        .expect("computer_hub_mcp_retries_attempted_total must register once")
    });

    static MCP_RETRIES_SUCCEEDED_TOTAL: LazyLock<IntCounter> = LazyLock::new(|| {
        register_int_counter!(
            "computer_hub_mcp_retries_succeeded_total",
            "MCP calls that succeeded after at least one retry."
        )
        .expect("computer_hub_mcp_retries_succeeded_total must register once")
    });

    static MCP_RETRIES_EXHAUSTED_TOTAL: LazyLock<IntCounter> = LazyLock::new(|| {
        register_int_counter!(
            "computer_hub_mcp_retries_exhausted_total",
            "MCP calls that exhausted retry budget and still failed."
        )
        .expect("computer_hub_mcp_retries_exhausted_total must register once")
    });

    static MCP_NON_RETRYABLE_FAILURES_TOTAL: LazyLock<IntCounter> = LazyLock::new(|| {
        register_int_counter!(
            "computer_hub_mcp_non_retryable_failures_total",
            "MCP call failures classified as non-retryable by the adapter."
        )
        .expect("computer_hub_mcp_non_retryable_failures_total must register once")
    });

    static MCP_TOOLS_BRIDGED: LazyLock<IntGauge> = LazyLock::new(|| {
        register_int_gauge!(
            "computer_hub_mcp_tools_bridged",
            "MCP tools currently bridged into the computer hub."
        )
        .expect("computer_hub_mcp_tools_bridged must register once")
    });

    pub(crate) fn mcp_call_duration_observe(secs: f64) {
        MCP_CALL_DURATION_SECONDS.observe(secs);
    }

    pub(crate) fn mcp_error() {
        MCP_ERRORS_TOTAL.inc();
    }

    pub(crate) fn mcp_call_timed_out() {
        MCP_CALL_TIMEOUTS_TOTAL.inc();
        #[cfg(test)]
        crate::metrics::test_hooks::mcp_timeout_inc();
    }

    pub(crate) fn mcp_retry_attempted() {
        MCP_RETRIES_ATTEMPTED_TOTAL.inc();
        #[cfg(test)]
        crate::metrics::test_hooks::mcp_retry_attempted_inc();
    }

    pub(crate) fn mcp_retry_succeeded() {
        MCP_RETRIES_SUCCEEDED_TOTAL.inc();
        #[cfg(test)]
        crate::metrics::test_hooks::mcp_retry_succeeded_inc();
    }

    pub(crate) fn mcp_retry_exhausted() {
        MCP_RETRIES_EXHAUSTED_TOTAL.inc();
        #[cfg(test)]
        crate::metrics::test_hooks::mcp_retry_exhausted_inc();
    }

    pub(crate) fn mcp_non_retryable_failure() {
        MCP_NON_RETRYABLE_FAILURES_TOTAL.inc();
        #[cfg(test)]
        crate::metrics::test_hooks::mcp_non_retryable_failure_inc();
    }

    pub(crate) fn mcp_tools_bridged_set(count: i64) {
        MCP_TOOLS_BRIDGED.set(count);
    }
}

#[cfg(not(feature = "metrics"))]
mod inner {
    pub(crate) fn mcp_call_duration_observe(_secs: f64) {}
    pub(crate) fn mcp_error() {}
    pub(crate) fn mcp_call_timed_out() {
        #[cfg(test)]
        crate::metrics::test_hooks::mcp_timeout_inc();
    }
    pub(crate) fn mcp_retry_attempted() {
        #[cfg(test)]
        crate::metrics::test_hooks::mcp_retry_attempted_inc();
    }
    pub(crate) fn mcp_retry_succeeded() {
        #[cfg(test)]
        crate::metrics::test_hooks::mcp_retry_succeeded_inc();
    }
    pub(crate) fn mcp_retry_exhausted() {
        #[cfg(test)]
        crate::metrics::test_hooks::mcp_retry_exhausted_inc();
    }
    pub(crate) fn mcp_non_retryable_failure() {
        #[cfg(test)]
        crate::metrics::test_hooks::mcp_non_retryable_failure_inc();
    }
    pub(crate) fn mcp_tools_bridged_set(_count: i64) {}
}

pub(crate) use inner::*;

#[cfg(test)]
pub(crate) use test_hooks::mcp_timeout_metric_hooks_snapshot_for_tests;
#[cfg(test)]
pub(crate) use test_hooks::reset_mcp_timeout_metric_hooks_for_tests;

#[cfg(test)]
mod test_hooks {
    use std::sync::atomic::{AtomicU64, Ordering};

    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    pub(crate) struct McpTimeoutMetricHookSnapshot {
        pub timeouts: u64,
        pub retries_attempted: u64,
        pub retries_succeeded: u64,
        pub retries_exhausted: u64,
        pub non_retryable_failures: u64,
    }

    static MCP_TIMEOUT_HOOKS: AtomicU64 = AtomicU64::new(0);
    static MCP_RETRY_ATTEMPTED_HOOKS: AtomicU64 = AtomicU64::new(0);
    static MCP_RETRY_SUCCEEDED_HOOKS: AtomicU64 = AtomicU64::new(0);
    static MCP_RETRY_EXHAUSTED_HOOKS: AtomicU64 = AtomicU64::new(0);
    static MCP_NON_RETRYABLE_HOOKS: AtomicU64 = AtomicU64::new(0);

    pub(crate) fn mcp_timeout_inc() {
        MCP_TIMEOUT_HOOKS.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn mcp_retry_attempted_inc() {
        MCP_RETRY_ATTEMPTED_HOOKS.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn mcp_retry_succeeded_inc() {
        MCP_RETRY_SUCCEEDED_HOOKS.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn mcp_retry_exhausted_inc() {
        MCP_RETRY_EXHAUSTED_HOOKS.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn mcp_non_retryable_failure_inc() {
        MCP_NON_RETRYABLE_HOOKS.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn reset_mcp_timeout_metric_hooks_for_tests() {
        MCP_TIMEOUT_HOOKS.store(0, Ordering::Relaxed);
        MCP_RETRY_ATTEMPTED_HOOKS.store(0, Ordering::Relaxed);
        MCP_RETRY_SUCCEEDED_HOOKS.store(0, Ordering::Relaxed);
        MCP_RETRY_EXHAUSTED_HOOKS.store(0, Ordering::Relaxed);
        MCP_NON_RETRYABLE_HOOKS.store(0, Ordering::Relaxed);
    }

    pub(crate) fn mcp_timeout_metric_hooks_snapshot_for_tests() -> McpTimeoutMetricHookSnapshot {
        McpTimeoutMetricHookSnapshot {
            timeouts: MCP_TIMEOUT_HOOKS.load(Ordering::Relaxed),
            retries_attempted: MCP_RETRY_ATTEMPTED_HOOKS.load(Ordering::Relaxed),
            retries_succeeded: MCP_RETRY_SUCCEEDED_HOOKS.load(Ordering::Relaxed),
            retries_exhausted: MCP_RETRY_EXHAUSTED_HOOKS.load(Ordering::Relaxed),
            non_retryable_failures: MCP_NON_RETRYABLE_HOOKS.load(Ordering::Relaxed),
        }
    }
}
