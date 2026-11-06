//! The load lifecycle shared by pages that show a background-loaded snapshot: ticketed
//! single-flight queries, stale results dropped, a failure keeping the last data.

use gpui_kit::*;

use super::query_state::{Finish, QueryState, Snapshot, apply_snapshot};

/// A page's snapshot together with the state of the query that refreshes it.
pub struct PageQuery<T> {
    name: &'static str,
    queries: QueryState,
    /// Swapped wholesale when a result applies; render only reads it.
    pub data: T,
}

impl<T> PageQuery<T> {
    pub fn new(name: &'static str, data: T) -> Self {
        Self { name, queries: QueryState::new(), data }
    }

    pub fn is_active(&self) -> bool {
        self.queries.is_active()
    }

    pub fn set_active(&mut self, active: bool) {
        self.queries.set_active(active);
    }
}

pub trait QueryPage: Sized + 'static {
    type Data: PartialEq + 'static;

    fn page_query(&mut self) -> &mut PageQuery<Self::Data>;

    /// Starts one query under the page's current inputs.
    fn fetch(&self) -> impl Future<Output = anyhow::Result<Self::Data>> + 'static;

    /// Runs after every applied result, including a failed one.
    fn applied(&mut self, _cx: &mut Context<Self>) {}

    /// Same-input reload (poll tick, action completion, page entry). Collapses into the
    /// running query; its result still applies when it lands.
    fn load(&mut self, cx: &mut Context<Self>) {
        if let Some(ticket) = self.page_query().queries.request() {
            self.launch(ticket, cx);
        }
    }

    /// The inputs changed: any result still running under the old inputs is stale and will
    /// be dropped when it lands.
    fn changed(&mut self, cx: &mut Context<Self>) {
        if let Some(ticket) = self.page_query().queries.restart() {
            self.launch(ticket, cx);
        }
    }

    fn launch(&mut self, ticket: u64, cx: &mut Context<Self>) {
        let fetch = self.fetch();
        cx.spawn(async move |this, cx| {
            let result = fetch.await;
            let _ = this.update(cx, |page, cx| page.loaded(ticket, result, cx));
        })
        .detach();
    }

    fn loaded(&mut self, ticket: u64, result: anyhow::Result<Self::Data>, cx: &mut Context<Self>) {
        match self.page_query().queries.finish(ticket) {
            Finish::Apply => self.apply(result, cx),
            Finish::ApplyAndReload(next) => {
                self.apply(result, cx);
                self.launch(next, cx);
            }
            Finish::Superseded(next) => self.launch(next, cx),
            Finish::Ignored => {}
        }
    }

    fn apply(&mut self, result: anyhow::Result<Self::Data>, cx: &mut Context<Self>) {
        let query = self.page_query();
        match apply_snapshot(&mut query.data, result) {
            Snapshot::Replaced => cx.notify(),
            Snapshot::Unchanged => {}
            Snapshot::Failed(e) => tracing::warn!("{} query failed: {e:#}", query.name),
        }
        self.applied(cx);
    }
}
