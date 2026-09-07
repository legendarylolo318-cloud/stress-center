/* stress_page/mod.rs
 *
 * Copyright 2026 Stress Center Contributors
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <http://www.gnu.org/licenses/>.
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

//! The Stress page: drives `stress-ng` from this (unprivileged) UI process
//! and shows CPU/memory load, package power and temperature while it runs,
//! reusing the same `GraphWidget` the Performance page's CPU tab uses.
//!
//! The gatherer (`magpie`) only ever reports per-core *usage*, not per-core
//! *clock speed* -- there is no such field in `magpie-types`. Rather than
//! touch the gatherer (a separate, pinned subproject) to add one, the
//! "per-core" graph here shows per-core utilization, which is the closest
//! per-core telemetry actually available.

use std::cell::{Cell, RefCell};

use adw::{prelude::*, subclass::prelude::*};
use gtk::glib::g_critical;
use gtk::{gio, glib};

use crate::i18n::i18n;
use crate::magpie_client::Readings;
use crate::performance_page::widgets::{
    AnimationFrame, DatasetGroup, FillingSettings, GraphWidget, GraphWidgetSettingsExt,
    RoundingSettings, ScalingSettings,
};
use crate::settings;

mod runner;

pub use runner::kill_active_group_now;

const TEMPERATURE_HIGH_WATERMARK: f32 = 45.;
const TEMPERATURE_LOW_WATERMARK: f32 = 35.;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LogKind {
    Info,
    Stdout,
    Stderr,
    Fail,
}

thread_local! {
    // Background threads (stdout/stderr readers, the wait thread) can't carry
    // a GTK widget reference across the thread boundary (GObject wrappers
    // aren't Send). Instead they post a plain-data closure to the main loop
    // via `glib::idle_add_once`, and that closure looks the page up here --
    // this is only ever touched from the main thread, both to set it (once,
    // at construction) and to read it (from those idle callbacks).
    static PAGE_INSTANCE: RefCell<Option<glib::WeakRef<StressPage>>> = RefCell::new(None);
}

fn current_page() -> Option<StressPage> {
    PAGE_INSTANCE.with(|p| p.borrow().as_ref().and_then(glib::WeakRef::upgrade))
}

mod imp {
    use super::*;

    #[derive(gtk::CompositeTemplate)]
    #[template(resource = "/io/missioncenter/MissionCenter/ui/stress_page/page.ui")]
    pub struct StressPage {
        #[template_child]
        pub status_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub start_stop_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub missing_binary_banner: TemplateChild<adw::Banner>,

        #[template_child]
        pub test_type_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub workers_row: TemplateChild<adw::SpinRow>,
        #[template_child]
        pub duration_row: TemplateChild<adw::SpinRow>,
        #[template_child]
        pub cpu_method_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub verify_row: TemplateChild<adw::SwitchRow>,

        #[template_child]
        pub graph_cpu: TemplateChild<GraphWidget>,
        #[template_child]
        pub graph_cores: TemplateChild<GraphWidget>,
        #[template_child]
        pub graph_power: TemplateChild<GraphWidget>,
        #[template_child]
        pub graph_temp: TemplateChild<GraphWidget>,
        #[template_child]
        pub power_graph_box: TemplateChild<gtk::Box>,
        #[template_child]
        pub temp_graph_box: TemplateChild<gtk::Box>,

        #[template_child]
        pub log_view: TemplateChild<gtk::TextView>,
        #[template_child]
        pub log_scroller: TemplateChild<gtk::ScrolledWindow>,

        pub is_running: Cell<bool>,
        pub cpu_count: Cell<usize>,
        pub has_power: Cell<bool>,
        pub has_temperature: Cell<bool>,
    }

    impl Default for StressPage {
        fn default() -> Self {
            Self {
                status_label: Default::default(),
                start_stop_button: Default::default(),
                missing_binary_banner: Default::default(),

                test_type_row: Default::default(),
                workers_row: Default::default(),
                duration_row: Default::default(),
                cpu_method_row: Default::default(),
                verify_row: Default::default(),

                graph_cpu: Default::default(),
                graph_cores: Default::default(),
                graph_power: Default::default(),
                graph_temp: Default::default(),
                power_graph_box: Default::default(),
                temp_graph_box: Default::default(),

                log_view: Default::default(),
                log_scroller: Default::default(),

                is_running: Cell::new(false),
                cpu_count: Cell::new(1),
                has_power: Cell::new(true),
                has_temperature: Cell::new(true),
            }
        }
    }

    impl StressPage {
        fn setup_graph(&self, graph: &GraphWidget, primary: DatasetGroup) {
            let mut active = DatasetGroup::new();
            active.dataset_settings.scaling_settings = ScalingSettings::Fixed;
            active.dataset_settings.high_watermark = 1.;
            active.dataset_settings.low_watermark = 0.;
            active.dataset_settings.fill = FillingSettings::FillToBottom;
            active.dataset_settings.dashed = false;
            active.dataset_settings.opacity = 55. / 255.;

            graph.add_dataset(primary);
            graph.add_dataset(active);
            graph.connect_to_settings(&settings!());
        }

        pub(super) fn setup_graphs(&self) {
            let cpu_count = self.cpu_count.get().max(1);

            let mut usage = DatasetGroup::new();
            usage.dataset_settings.scaling_settings = ScalingSettings::Fixed;
            usage.dataset_settings.high_watermark = 100.;
            self.setup_graph(&self.graph_cpu, usage);

            let mut per_core = DatasetGroup::new();
            per_core.dataset_settings.scaling_settings = ScalingSettings::Fixed;
            per_core.dataset_settings.high_watermark = 100.;
            per_core.dataset_settings.opacity = 100. / 255. / cpu_count as f32;
            per_core.set_datasets(cpu_count);
            self.setup_graph(&self.graph_cores, per_core);

            let mut power = DatasetGroup::new();
            power.dataset_settings.scaling_settings = ScalingSettings::StickyUp;
            power.dataset_settings.rounding_settings = RoundingSettings::Integer;
            power.dataset_settings.high_watermark = 0.;
            self.setup_graph(&self.graph_power, power);

            let mut temp = DatasetGroup::new();
            temp.dataset_settings.scaling_settings = ScalingSettings::StickyUpDown;
            temp.dataset_settings.rounding_settings = RoundingSettings::Integer;
            temp.dataset_settings.high_watermark = TEMPERATURE_HIGH_WATERMARK;
            temp.dataset_settings.low_watermark = TEMPERATURE_LOW_WATERMARK;
            self.setup_graph(&self.graph_temp, temp);
        }

        fn configure_log_tags(&self) {
            let buffer = self.log_view.buffer();

            let _ = buffer.create_tag(Some("stress-log-info"), &[("foreground", &"#5e5c64")]);
            let _ = buffer.create_tag(Some("stress-log-stderr"), &[("foreground", &"#e5a50a")]);
            let _ = buffer.create_tag(
                Some("stress-log-fail"),
                &[("foreground", &"#ffffff"), ("background", &"#c01c28")],
            );
        }

        fn populate_cpu_methods(&self) {
            std::thread::spawn(move || {
                let methods = runner::cpu_methods();
                glib::idle_add_once(move || {
                    let Some(this) = super::current_page() else {
                        return;
                    };
                    let imp = this.imp();

                    let strings: Vec<&str> = methods.iter().map(|s| s.as_str()).collect();
                    let model = gtk::StringList::new(&strings);
                    let default_idx = methods.iter().position(|m| m == "all").unwrap_or(0);

                    imp.cpu_method_row.set_model(Some(&model));
                    imp.cpu_method_row.set_selected(default_idx as u32);
                });
            });
        }

        fn check_availability(&self) {
            std::thread::spawn(move || {
                let available = runner::is_available();
                glib::idle_add_once(move || {
                    let Some(this) = super::current_page() else {
                        return;
                    };
                    let imp = this.imp();

                    if !available {
                        imp.missing_binary_banner.set_title(&i18n(
                            "stress-ng was not found in PATH. Install it to use this page.",
                        ));
                        imp.missing_binary_banner.set_revealed(true);
                        imp.start_stop_button.set_sensitive(false);
                    }
                });
            });
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for StressPage {
        const NAME: &'static str = "StressPage";
        type Type = super::StressPage;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for StressPage {
        fn constructed(&self) {
            self.parent_constructed();

            PAGE_INSTANCE.with(|p| *p.borrow_mut() = Some(self.obj().downgrade()));

            self.workers_row.set_value(
                std::thread::available_parallelism()
                    .map(|n| n.get())
                    .unwrap_or(1) as f64,
            );

            self.configure_log_tags();
            self.populate_cpu_methods();
            self.check_availability();

            self.start_stop_button.connect_clicked({
                let this = self.obj().downgrade();
                move |_| {
                    let Some(this) = this.upgrade() else {
                        return;
                    };

                    if this.imp().is_running.get() {
                        this.stop_test();
                    } else {
                        this.start_test();
                    }
                }
            });
        }
    }

    impl WidgetImpl for StressPage {}

    impl BoxImpl for StressPage {}
}

glib::wrapper! {
    pub struct StressPage(ObjectSubclass<imp::StressPage>)
        @extends gtk::Box, gtk::Widget,
        @implements gio::ActionGroup, gio::ActionMap, gtk::ConstraintTarget, gtk::Accessible, gtk::Buildable;
}

impl StressPage {
    fn set_controls_sensitive(&self, sensitive: bool) {
        let imp = self.imp();
        imp.test_type_row.set_sensitive(sensitive);
        imp.workers_row.set_sensitive(sensitive);
        imp.duration_row.set_sensitive(sensitive);
        imp.cpu_method_row.set_sensitive(sensitive);
        imp.verify_row.set_sensitive(sensitive);
    }

    fn clear_log(&self) {
        let buffer = self.imp().log_view.buffer();
        buffer.set_text("");
    }

    pub(super) fn append_log_line(&self, line: &str, kind: LogKind) {
        let imp = self.imp();
        let buffer = imp.log_view.buffer();

        let mut end = buffer.end_iter();
        let text = format!("{line}\n");

        match kind {
            LogKind::Stdout => buffer.insert(&mut end, &text),
            LogKind::Info => {
                buffer.insert_with_tags_by_name(&mut end, &text, &["stress-log-info"])
            }
            LogKind::Stderr => {
                buffer.insert_with_tags_by_name(&mut end, &text, &["stress-log-stderr"])
            }
            LogKind::Fail => {
                buffer.insert_with_tags_by_name(&mut end, &text, &["stress-log-fail"])
            }
        }

        let mut end = buffer.end_iter();
        let mark = buffer.create_mark(None, &mut end, false);
        imp.log_view.scroll_mark_onscreen(&mark);
    }

    fn start_test(&self) {
        let imp = self.imp();
        if imp.is_running.get() {
            return;
        }

        if !runner::is_available() {
            imp.missing_binary_banner.set_title(&i18n(
                "stress-ng was not found in PATH. Install it to use this page.",
            ));
            imp.missing_binary_banner.set_revealed(true);
            return;
        }
        imp.missing_binary_banner.set_revealed(false);

        let test_type = imp.test_type_row.selected();
        let workers = imp.workers_row.value().round().max(1.) as u32;
        let duration_secs = imp.duration_row.value().round().max(1.) as u32;
        let verify = imp.verify_row.is_active();
        let cpu_method = imp
            .cpu_method_row
            .selected_item()
            .and_downcast::<gtk::StringObject>()
            .map(|s| s.string().to_string());

        let config = runner::StressConfig {
            test_cpu: test_type == 0 || test_type == 2,
            test_memory: test_type == 1 || test_type == 2,
            workers,
            duration_secs,
            cpu_method,
            verify,
        };

        self.clear_log();
        self.append_log_line(
            &format!("$ stress-ng {}", config.command_line_preview()),
            LogKind::Info,
        );

        match runner::spawn(config) {
            Ok(()) => {
                imp.is_running.set(true);
                imp.start_stop_button.set_label(&i18n("Stop"));
                imp.start_stop_button.remove_css_class("suggested-action");
                imp.start_stop_button.add_css_class("destructive-action");
                imp.status_label.set_text(&i18n("Running…"));
                self.set_controls_sensitive(false);
            }
            Err(e) => {
                self.append_log_line(
                    &format!("Failed to start stress-ng: {}", e),
                    LogKind::Fail,
                );
                g_critical!("StressCenter::StressPage", "Failed to start stress-ng: {}", e);
            }
        }
    }

    fn stop_test(&self) {
        let imp = self.imp();
        if !imp.is_running.get() {
            return;
        }

        runner::stop();
        imp.status_label.set_text(&i18n("Stopping…"));
        imp.start_stop_button.set_sensitive(false);
    }

    pub(super) fn on_process_exited(&self, status: std::io::Result<std::process::ExitStatus>) {
        let imp = self.imp();

        imp.is_running.set(false);
        imp.start_stop_button.set_label(&i18n("Start"));
        imp.start_stop_button.remove_css_class("destructive-action");
        imp.start_stop_button.add_css_class("suggested-action");
        imp.start_stop_button.set_sensitive(true);
        self.set_controls_sensitive(true);

        let message = match status {
            Ok(status) if status.success() => i18n("Finished"),
            Ok(status) => format!("{} ({})", i18n("Finished with errors"), status),
            Err(e) => format!("{}: {}", i18n("Error waiting for stress-ng"), e),
        };

        imp.status_label.set_text(&message);
        self.append_log_line(&format!("-- {} --", message), LogKind::Info);
    }

    pub fn set_static_information(&self, readings: &Readings) -> bool {
        let imp = self.imp();

        imp.cpu_count.set(readings.cpu.core_usage_percent.len().max(1));
        imp.has_power.set(readings.cpu.power_draw_w.is_some());
        imp.has_temperature.set(readings.cpu.temperature_celsius.is_some());

        imp.power_graph_box.set_visible(imp.has_power.get());
        imp.temp_graph_box.set_visible(imp.has_temperature.get());

        // Deferred until here (rather than done in `constructed()`) because
        // the per-core dataset group needs the real core count to size
        // itself; it isn't known until the first readings arrive.
        imp.setup_graphs();

        true
    }

    pub fn update_readings(&self, readings: &Readings) -> bool {
        let imp = self.imp();

        let active = if imp.is_running.get() { 1. } else { 0. };
        let cpu = &readings.cpu;

        imp.graph_cpu
            .add_data_point(vec![vec![cpu.total_usage_percent], vec![active]]);

        imp.graph_cores
            .add_data_point(vec![cpu.core_usage_percent.clone(), vec![active]]);

        if imp.has_power.get() {
            imp.graph_power.add_data_point(vec![
                vec![cpu.power_draw_w.unwrap_or(0.)],
                vec![active],
            ]);
        }

        if imp.has_temperature.get() {
            imp.graph_temp.add_data_point(vec![
                vec![cpu.temperature_celsius.unwrap_or(0.)],
                vec![active],
            ]);
        }

        true
    }

    pub fn update_animations(&self, ticks: AnimationFrame) -> bool {
        let imp = self.imp();

        imp.graph_cpu.update_animation(ticks);
        imp.graph_cores.update_animation(ticks);
        imp.graph_power.update_animation(ticks);
        imp.graph_temp.update_animation(ticks);

        true
    }
}
