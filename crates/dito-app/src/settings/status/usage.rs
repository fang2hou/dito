//! CPU and memory on this Mac: the app, each local model in use, and the total. Read only while the Status page is
//! on screen.

use std::time::{Duration, Instant};

use dito_core::config::Source;
use dito_core::factory::LocalBackend;
use dito_core::health::EngineStatus;
use dito_core::i18n::Tr;
use dito_core::secrets::Role;
use dito_local::{EngineUsage, ProcessUsage, process_usage};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::settings::Ctx;
use crate::state::AppState;
use crate::ui::{self, text};

pub(super) const SAMPLE_EVERY: Duration = Duration::from_secs(2);

struct ModelReading {
    role: Role,
    id: String,
    name: String,
    usage: EngineUsage,
}

struct Reading {
    at: Instant,
    process: ProcessUsage,
    models: Vec<ModelReading>,
}

impl Reading {
    /// A cloud model runs elsewhere and a stopped one holds nothing, so neither has a row.
    fn take(st: &AppState) -> Self {
        let cfg = st.cfg.get();
        let roles = [(Role::Asr, &cfg.asr, true), (Role::Text, &cfg.text_model, cfg.polish.enabled)];
        let models = roles
            .into_iter()
            .filter(|(_, engine, enabled)| *enabled && engine.source == Source::Local)
            .filter(|(_, engine, _)| st.local.status(&engine.local_model) == EngineStatus::Running)
            .map(|(role, engine, _)| {
                let id = engine.local_model.clone();
                let name = st.models.spec(&id).map_or_else(|| id.clone(), |spec| spec.name.clone());
                ModelReading { role, usage: st.local.usage(&id), id, name }
            })
            .collect();
        let mut process = process_usage();
        let workers = st.local.worker_usage();
        process.cpu = process.cpu.saturating_add(workers.cpu);
        process.memory_bytes = process.memory_bytes.saturating_add(workers.memory_bytes);
        Self { at: Instant::now(), process, models }
    }
}

struct Line {
    /// Percent of one core; `None` until two readings exist.
    cpu: Option<f32>,
    memory_bytes: u64,
}

struct Table {
    app: Line,
    models: Vec<(Role, String, Line)>,
    total: Line,
}

impl Table {
    fn between(before: Option<&Reading>, now: &Reading) -> Self {
        let percent = |spent: Duration, since: &Reading| {
            let elapsed = now.at.duration_since(since.at).as_secs_f32();
            (elapsed > 0.).then(|| spent.as_secs_f32() / elapsed * 100.)
        };
        let total = Line {
            cpu: before.and_then(|b| percent(now.process.cpu.saturating_sub(b.process.cpu), b)),
            memory_bytes: now
                .process
                .memory_bytes
                .saturating_add(now.models.iter().map(|m| m.usage.mapped_bytes).sum()),
        };
        let models: Vec<_> = now
            .models
            .iter()
            .map(|model| {
                let cpu = before.and_then(|b| {
                    let earlier = b.models.iter().find(|m| m.id == model.id)?;
                    percent(model.usage.cpu.saturating_sub(earlier.usage.cpu), b)
                });
                (model.role, model.name.clone(), Line { cpu, memory_bytes: model.usage.memory_bytes })
            })
            .collect();
        let model_cpu: f32 = models.iter().filter_map(|(_, _, line)| line.cpu).sum();
        let model_memory: u64 = models.iter().map(|(_, _, line)| line.memory_bytes).sum();
        let app = Line {
            cpu: total.cpu.map(|cpu| (cpu - model_cpu).max(0.)),
            memory_bytes: total.memory_bytes.saturating_sub(model_memory),
        };
        Self { app, models, total }
    }
}

#[derive(Default)]
pub(super) struct UsageMeter {
    last: Option<Reading>,
    shown: Option<Table>,
}

impl UsageMeter {
    pub(super) fn sample(&mut self, st: &AppState) {
        let now = Reading::take(st);
        self.shown = Some(Table::between(self.last.as_ref(), &now));
        self.last = Some(now);
    }

    pub(super) fn stop(&mut self) {
        *self = Self::default();
    }

    pub(super) fn table(&self, c: &Ctx) -> Option<Div> {
        let table = self.shown.as_ref()?;
        let p = c.p;
        let divider = || div().w_full().h(px(1.)).bg(p.border);
        let header = cells(
            text(c.t(Tr::StatusUsageTitle), 13., FontWeight::MEDIUM, p.muted).line_height(px(20.)),
            ["CPU".into(), c.t(Tr::ModelsMemory)],
            p.muted,
        )
        .py(px(12.))
        .rounded_t(px(15.))
        .bg(p.sub);
        let row = |title: SharedString, detail: Option<SharedString>, line: &Line| {
            let name = div()
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(text(title, 15., FontWeight::MEDIUM, p.text).line_height(px(22.)))
                .children(detail.map(|detail| text(detail, 13., FontWeight::NORMAL, p.muted).line_height(px(20.))));
            let cpu = line.cpu.map_or_else(|| "—".to_owned(), |cpu| format!("{cpu:.1}%"));
            cells(name, [cpu.into(), ui::fmt_bytes(line.memory_bytes).into()], p.text).py(px(14.))
        };
        let mut card = ui::card(&p).child(header).child(divider()).child(row(
            "Dito".into(),
            Some(c.t(Tr::StatusUsageApp)),
            &table.app,
        ));
        for (role, name, line) in &table.models {
            let detail = match role {
                Role::Asr => Tr::StatusUsageAsrModel,
                Role::Text => Tr::StatusUsageTextModel,
            };
            card = card.child(divider()).child(row(name.clone().into(), Some(c.t(detail)), line));
        }
        Some(card.child(divider()).child(row(c.t(Tr::StatusUsageTotal), None, &table.total)))
    }
}

fn cells(name: Div, values: [SharedString; 2], color: Hsla) -> Div {
    div().w_full().px(px(24.)).flex().items_center().gap(px(16.)).child(name.flex_1().min_w(px(0.))).children(
        values.map(|value| text(value, 13., FontWeight::MEDIUM, color).w(px(96.)).flex_none().line_height(px(20.))),
    )
}
