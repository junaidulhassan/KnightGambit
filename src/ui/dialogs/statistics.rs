use std::cell::RefCell;
use std::rc::{Rc, Weak};

use adw::prelude::*;
use gtk::glib;

use crate::history::stats::{
    ErrorPatterns, Insight, InsightKind, MIN_SAMPLE, MoveQuality, PIECE_LABELS, Phase, ROLLING_WINDOW, Stats, Tally,
};
use crate::history::{Outcome, compute_stats};
use crate::review::{CLASS_COUNT, MoveClass};
use crate::ui::charts::{
    self, ACCENT, DRAW, HBar, HeatCell, HeatRow, LOSS, LineSeries, LineSpec, RadarPoint, Slice, StackRow, WIN,
};
use crate::ui::controller::Controller;
use crate::ui::dialogs::history::{format_date, format_duration, format_short_date};

struct StatsView {
    controller: Rc<Controller>,
    dialog: adw::Dialog,
    stack: adw::ViewStack,
    root: gtk::Stack,
    pages: RefCell<Vec<gtk::Widget>>,
    revision: std::cell::Cell<u64>,
}

pub fn present(controller: &Rc<Controller>) {
    let stack = adw::ViewStack::new();
    let switcher = adw::ViewSwitcher::builder().stack(&stack).policy(adw::ViewSwitcherPolicy::Wide).build();
    let switcher_bar = adw::ViewSwitcherBar::builder().stack(&stack).build();
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&switcher));

    let spinner = adw::StatusPage::builder().title("Crunching your games…").build();
    let spinner_widget = gtk::Spinner::new();
    spinner_widget.set_spinning(true);
    spinner_widget.set_size_request(32, 32);
    spinner.set_child(Some(&spinner_widget));
    let empty = adw::StatusPage::builder()
        .icon_name("kg-stats-symbolic")
        .title("No statistics yet")
        .description("Finish a game against the computer and your performance analytics will appear here.")
        .build();
    let root = gtk::Stack::new();
    root.add_named(&spinner, Some("loading"));
    root.add_named(&empty, Some("empty"));
    root.add_named(&stack, Some("stats"));

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.add_bottom_bar(&switcher_bar);
    toolbar.set_content(Some(&root));

    let bin = adw::BreakpointBin::new();
    bin.set_width_request(360);
    bin.set_height_request(480);
    bin.set_child(Some(&toolbar));
    let breakpoint = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
        adw::BreakpointConditionLengthType::MaxWidth,
        640.0,
        adw::LengthUnit::Sp,
    ));
    breakpoint.add_setter(&switcher_bar, "reveal", Some(&true.to_value()));
    breakpoint.add_setter(&switcher, "visible", Some(&false.to_value()));
    bin.add_breakpoint(breakpoint);

    let dialog = adw::Dialog::builder()
        .title("Statistics & Analytics")
        .content_width(1100)
        .content_height(780)
        .child(&bin)
        .build();
    let view = Rc::new(StatsView {
        controller: controller.clone(),
        dialog,
        stack,
        root,
        pages: RefCell::new(Vec::new()),
        revision: std::cell::Cell::new(0),
    });
    let keeper = Rc::new(RefCell::new(Some(view.clone())));
    view.dialog.connect_closed(move |_| {
        keeper.borrow_mut().take();
    });
    let weak = Rc::downgrade(&view);
    controller.connect_history_changed(move || match weak.upgrade() {
        Some(view) => {
            if view.dialog.is_mapped() {
                view.reload(false);
            }
            true
        }
        None => false,
    });
    view.dialog.present(Some(&controller.ui().window));
    view.reload(true);
}

impl StatsView {
    fn reload(self: &Rc<Self>, show_loading: bool) {
        if show_loading {
            self.root.set_visible_child_name("loading");
        }
        let revision = self.revision.get() + 1;
        self.revision.set(revision);
        let store = self.controller.history().clone();
        let weak: Weak<Self> = Rc::downgrade(self);
        self.controller.worker().run(
            move || compute_stats(&store),
            move |result| {
                let Some(view) = weak.upgrade() else { return };
                if view.revision.get() != revision {
                    return;
                }
                match result {
                    Ok(stats) => view.show(&stats),
                    Err(error) => view.controller.toast(&format!("Could not compute statistics: {error}")),
                }
            },
        );
    }

    fn show(&self, stats: &Stats) {
        if stats.total_games() == 0 {
            self.root.set_visible_child_name("empty");
            return;
        }
        let selected = self.stack.visible_child_name();
        for page in self.pages.borrow_mut().drain(..) {
            self.stack.remove(&page);
        }
        let pages: Vec<(&str, &str, &str, gtk::Widget)> = vec![
            ("overview", "Overview", "kg-stats-symbolic", overview_page(stats)),
            ("openings", "Openings", "kg-book-symbolic", openings_page(stats)),
            ("quality", "Move Quality", "kg-review-symbolic", quality_page(stats)),
            ("insights", "Strengths & Weaknesses", "kg-trophy-symbolic", insights_page(stats)),
        ];
        for (name, title, icon, widget) in pages {
            self.stack.add_titled_with_icon(&widget, Some(name), title, icon);
            self.pages.borrow_mut().push(widget);
        }
        if let Some(name) = selected {
            self.stack.set_visible_child_name(&name);
        }
        self.root.set_visible_child_name("stats");
    }
}

// ------------------------------------------------------------- building ----

fn percent(value: f64) -> String {
    format!("{value:.0}%")
}

fn plural(count: usize, word: &str) -> String {
    format!("{count} {word}{}", if count == 1 { "" } else { "s" })
}

fn record_text(tally: &Tally) -> String {
    format!("{}W · {}D · {}L", tally.wins, tally.draws, tally.losses)
}

fn page(children: Vec<gtk::Widget>) -> gtk::Widget {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 14);
    column.set_margin_top(14);
    column.set_margin_bottom(20);
    column.set_margin_start(16);
    column.set_margin_end(16);
    for child in children {
        column.append(&child);
    }
    let clamp = adw::Clamp::builder().maximum_size(1040).tightening_threshold(760).child(&column).build();
    gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .child(&clamp)
        .build()
        .upcast()
}

fn card(title: &str, subtitle: Option<&str>, content: &impl IsA<gtk::Widget>) -> gtk::Widget {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 6);
    card.add_css_class("card");
    card.add_css_class("stat-card");
    card.set_hexpand(true);
    let heading = gtk::Label::new(Some(title));
    heading.add_css_class("heading");
    heading.set_xalign(0.0);
    card.append(&heading);
    if let Some(subtitle) = subtitle {
        let caption = gtk::Label::new(Some(subtitle));
        caption.add_css_class("caption");
        caption.add_css_class("dim-label");
        caption.set_xalign(0.0);
        caption.set_wrap(true);
        card.append(&caption);
    }
    card.append(content);
    card.upcast()
}

fn note(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.add_css_class("dim-label");
    label.set_wrap(true);
    label.set_xalign(0.0);
    label
}

fn grid(children: Vec<gtk::Widget>, max_columns: u32) -> gtk::Widget {
    let flow = gtk::FlowBox::new();
    flow.set_selection_mode(gtk::SelectionMode::None);
    flow.set_homogeneous(true);
    flow.set_min_children_per_line(1);
    flow.set_max_children_per_line(max_columns);
    flow.set_row_spacing(14);
    flow.set_column_spacing(14);
    flow.set_activate_on_single_click(false);
    for child in children {
        let item = gtk::FlowBoxChild::new();
        item.set_focusable(false);
        item.set_child(Some(&child));
        flow.append(&item);
    }
    flow.upcast()
}

fn tile(value: &str, caption: &str, tone: Option<&str>) -> gtk::Widget {
    let tile = gtk::Box::new(gtk::Orientation::Vertical, 2);
    tile.add_css_class("card");
    tile.add_css_class("stat-tile");
    let number = gtk::Label::new(Some(value));
    number.add_css_class("stat-value");
    if let Some(tone) = tone {
        number.add_css_class(tone);
    }
    number.set_xalign(0.0);
    let label = gtk::Label::new(Some(caption));
    label.add_css_class("caption");
    label.add_css_class("dim-label");
    label.set_xalign(0.0);
    label.set_wrap(true);
    tile.append(&number);
    tile.append(&label);
    tile.upcast()
}

fn stack_row(label: String, right: String, tally: &Tally, subject: &str) -> StackRow {
    let tip = |kind: &str, count: usize| {
        format!(
            "{subject}\n{kind}: {count} of {} games ({:.0}%)\n{}",
            tally.games(),
            if tally.games() == 0 { 0.0 } else { count as f64 * 100.0 / tally.games() as f64 },
            record_text(tally)
        )
    };
    StackRow {
        label,
        right_text: right,
        counts: [tally.wins, tally.draws, tally.losses],
        tooltips: [tip("Wins", tally.wins), tip("Draws", tally.draws), tip("Losses", tally.losses)],
    }
}

fn legend() -> gtk::Widget {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 14);
    for (label, color) in [("Wins", WIN), ("Draws", DRAW), ("Losses", LOSS)] {
        let item = gtk::Label::new(None);
        item.set_markup(&format!("<span foreground=\"{color}\">●</span> {label}"));
        item.add_css_class("caption");
        row.append(&item);
    }
    row.upcast()
}

fn with_legend(chart: &impl IsA<gtk::Widget>) -> gtk::Widget {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 6);
    column.append(chart);
    column.append(&legend());
    column.upcast()
}

fn pick_labels(count: usize) -> Vec<usize> {
    match count {
        0 => Vec::new(),
        1 => vec![0],
        2 | 3 => vec![0, count - 1],
        _ => vec![0, count / 2, count - 1],
    }
}

// ------------------------------------------------------------- overview ----

fn overview_page(stats: &Stats) -> gtk::Widget {
    let Some(overall) = &stats.overall else { return page(Vec::new()) };
    let t = &overall.tally;
    let streak = match overall.current_streak {
        Some((Outcome::Win, n)) => format!("{n} win{}", if n == 1 { "" } else { "s" }),
        Some((Outcome::Loss, n)) => format!("{n} loss{}", if n == 1 { "" } else { "es" }),
        Some((Outcome::Draw, n)) => format!("{n} draw{}", if n == 1 { "" } else { "s" }),
        None => "—".to_owned(),
    };
    let tiles = grid(
        vec![
            tile(&t.games().to_string(), "Games played", None),
            tile(&t.wins.to_string(), "Wins", Some("win")),
            tile(&t.losses.to_string(), "Losses", Some("loss")),
            tile(&t.draws.to_string(), "Draws", Some("draw")),
            tile(&percent(t.win_rate()), "Win rate", Some("win")),
            tile(&percent(t.draw_rate()), "Draw rate", None),
            tile(
                &overall.avg_duration_secs.map_or("—".to_owned(), |secs| format_duration(secs as i64)),
                "Average game duration",
                None,
            ),
            tile(&format!("{:.0}", (overall.avg_plies / 2.0).ceil()), "Average moves per game", None),
            tile(&overall.longest_win_streak.to_string(), "Longest winning streak", Some("win")),
            tile(&overall.longest_loss_streak.to_string(), "Longest losing streak", Some("loss")),
            tile(&streak, "Current streak", None),
        ],
        6,
    );

    let donut = charts::donut(
        vec![
            Slice {
                label: "Wins".into(),
                value: t.wins as f64,
                color: WIN.into(),
                tooltip: format!("Wins: {} of {} games ({})", t.wins, t.games(), percent(t.win_rate())),
            },
            Slice {
                label: "Draws".into(),
                value: t.draws as f64,
                color: DRAW.into(),
                tooltip: format!("Draws: {} of {} games ({})", t.draws, t.games(), percent(t.draw_rate())),
            },
            Slice {
                label: "Losses".into(),
                value: t.losses as f64,
                color: LOSS.into(),
                tooltip: format!("Losses: {} of {} games ({})", t.losses, t.games(), percent(t.loss_rate())),
            },
        ],
        percent(t.win_rate()),
        "win rate".into(),
    );
    let results = card("Results", Some("All games against the computer"), &with_legend(&donut));

    let n = stats.trend.len();
    let trend_card = if n >= 2 {
        let tooltips = stats
            .trend
            .iter()
            .map(|point| {
                format!(
                    "Game {} · {}\nResult: {}\nWin rate over the last {}: {}\nOverall win rate: {}",
                    point.game_index,
                    format_date(point.played_at),
                    point.outcome.label(),
                    point.game_index.min(ROLLING_WINDOW),
                    percent(point.rolling_win_rate),
                    percent(point.cumulative_win_rate)
                )
            })
            .collect();
        let chart = charts::line_chart(LineSpec {
            series: vec![
                LineSeries {
                    name: format!("Last {ROLLING_WINDOW} games"),
                    color: ACCENT.into(),
                    values: stats.trend.iter().map(|p| p.rolling_win_rate).collect(),
                },
                LineSeries {
                    name: "Overall".into(),
                    color: charts::SERIES[1].into(),
                    values: stats.trend.iter().map(|p| p.cumulative_win_rate).collect(),
                },
            ],
            tooltips,
            x_labels: pick_labels(n).into_iter().map(|i| (i, format_short_date(stats.trend[i].played_at))).collect(),
            y_max: 100.0,
            y_suffix: "%",
            description: "Win rate trend over time".into(),
        });
        let key = gtk::Label::new(None);
        key.set_markup(&format!(
            "<span foreground=\"{ACCENT}\">●</span> Last {ROLLING_WINDOW} games   <span foreground=\"{}\">●</span> Overall",
            charts::SERIES[1]
        ));
        key.add_css_class("caption");
        key.set_xalign(0.0);
        let column = gtk::Box::new(gtk::Orientation::Vertical, 6);
        column.append(&chart);
        column.append(&key);
        card("Win-rate trend", Some("Hover a point for the game behind it"), &column)
    } else {
        card("Win-rate trend", None, &note("Play at least two games to see how your results develop over time."))
    };

    let color_rows = vec![
        stack_row("As White".into(), plural(stats.by_color[0].games(), "game"), &stats.by_color[0], "Playing White"),
        stack_row("As Black".into(), plural(stats.by_color[1].games(), "game"), &stats.by_color[1], "Playing Black"),
    ];
    let colors = card("White vs Black", None, &with_legend(&charts::stacked_bars(color_rows, "Results by playing color")));

    let level_rows: Vec<StackRow> = stats
        .by_level
        .iter()
        .map(|level| {
            let label = match level.elo {
                Some(elo) => format!("{} (~{elo})", level.label),
                None => level.label.clone(),
            };
            stack_row(label, plural(level.tally.games(), "game"), &level.tally, &format!("Against {}", level.label))
        })
        .collect();
    let levels = card(
        "Against each difficulty",
        Some("Computer strength level you played"),
        &with_legend(&charts::stacked_bars(level_rows, "Results by opponent difficulty")),
    );

    let progress = progress_card(stats);
    page(vec![tiles, grid(vec![results, trend_card], 2), grid(vec![colors, levels], 2), progress])
}

fn progress_card(stats: &Stats) -> gtk::Widget {
    let Some(progress) = &stats.progress else {
        return card(
            "Recent form",
            None,
            &note("Play at least six games to compare your recent results with the games before them."),
        );
    };
    let column = gtk::Box::new(gtk::Orientation::Vertical, 10);
    let row = |title: &str, value: f64, previous: f64, suffix: &str| {
        let delta = value - previous;
        let (arrow, tone) = if delta >= 1.0 {
            ("▲", WIN)
        } else if delta <= -1.0 {
            ("▼", LOSS)
        } else {
            ("●", DRAW)
        };
        let box_ = gtk::Box::new(gtk::Orientation::Vertical, 4);
        let line = gtk::Label::new(None);
        line.set_xalign(0.0);
        line.set_markup(&format!(
            "{title}  <b>{value:.0}{suffix}</b>   <span foreground=\"{tone}\">{arrow} {delta:+.0} points</span> <span alpha=\"60%\">vs {previous:.0}{suffix} before</span>"
        ));
        let bar = gtk::ProgressBar::new();
        bar.set_fraction((value / 100.0).clamp(0.0, 1.0));
        bar.add_css_class("progress-bar-wide");
        box_.append(&line);
        box_.append(&bar);
        box_
    };
    column.append(&row(
        &format!("Score, last {} games", progress.window),
        progress.recent_score,
        progress.previous_score,
        "%",
    ));
    if let (Some(recent), Some(previous)) = (progress.recent_accuracy, progress.previous_accuracy) {
        column.append(&row("Move accuracy, recent reviewed games", recent, previous, "%"));
    }
    card(
        "Recent form",
        Some("Your most recent games compared with the same number of games before them"),
        &column,
    )
}

// ------------------------------------------------------------- openings ----

const ECO_NAMES: [&str; 5] = [
    "A · Flank & unusual openings",
    "B · Semi-open (Sicilian, Caro-Kann…)",
    "C · Open games & French",
    "D · Closed & Queen's Gambit",
    "E · Indian defenses & Catalan",
];

fn opening_label(name: &str, eco: Option<&str>) -> String {
    match eco {
        Some(eco) => format!("{name} · {eco}"),
        None => name.to_owned(),
    }
}

fn openings_page(stats: &Stats) -> gtk::Widget {
    if stats.openings.is_empty() {
        let empty = adw::StatusPage::builder()
            .icon_name("kg-book-symbolic")
            .title("No openings recorded yet")
            .description("Openings are detected from the built-in opening book. Games that leave the book on the first move are not classified.")
            .build();
        return empty.upcast();
    }
    let top: Vec<_> = stats.openings.iter().take(10).collect();
    let played = charts::hbar_chart(
        top.iter()
            .map(|opening| HBar {
                label: opening_label(&opening.name, opening.eco.as_deref()),
                value: opening.tally.games() as f64,
                color: ACCENT.into(),
                value_text: opening.tally.games().to_string(),
                tooltip: format!(
                    "{}\n{} · {}\nWin {} · Draw {} · Loss {}\nWhite {} games · Black {} games",
                    opening.name,
                    plural(opening.tally.games(), "game"),
                    record_text(&opening.tally),
                    percent(opening.tally.win_rate()),
                    percent(opening.tally.draw_rate()),
                    percent(opening.tally.loss_rate()),
                    opening.as_white.games(),
                    opening.as_black.games()
                ),
            })
            .collect(),
        "Most frequently played openings",
    );
    let played = card("Most played openings", Some("Opening family and most common ECO code"), &played);

    let results = charts::stacked_bars(
        top.iter()
            .map(|opening| {
                stack_row(
                    opening_label(&opening.name, opening.eco.as_deref()),
                    plural(opening.tally.games(), "game"),
                    &opening.tally,
                    &opening.name,
                )
            })
            .collect(),
        "Win, draw and loss rates per opening",
    );
    let results = card("Results by opening", None, &with_legend(&results));

    let ranking = |best: bool| {
        let list = gtk::ListBox::new();
        list.add_css_class("boxed-list");
        list.set_selection_mode(gtk::SelectionMode::None);
        let openings = stats.ranked_openings(MIN_SAMPLE, best, 5);
        for opening in &openings {
            let row = adw::ActionRow::builder()
                .title(glib::markup_escape_text(&opening_label(&opening.name, opening.eco.as_deref())))
                .subtitle(format!("{} · {}", plural(opening.tally.games(), "game"), record_text(&opening.tally)))
                .build();
            let score = gtk::Label::new(Some(&percent(opening.tally.score_pct())));
            score.add_css_class("heading");
            score.add_css_class(if opening.tally.score_pct() >= 50.0 { "win" } else { "loss" });
            row.add_suffix(&score);
            list.append(&row);
        }
        if openings.is_empty() {
            let row = adw::ActionRow::builder().title(format!("Needs openings with at least {MIN_SAMPLE} games")).build();
            row.add_css_class("dim-label");
            list.append(&row);
        }
        list
    };
    let best = card("Best performing", Some("Score = wins plus half of draws"), &ranking(true));
    let worst = card("Worst performing", Some("Candidates for study or replacement"), &ranking(false));

    let by_color = |white: bool| {
        let mut openings: Vec<_> = stats
            .openings
            .iter()
            .filter(|o| if white { o.as_white.games() > 0 } else { o.as_black.games() > 0 })
            .collect();
        openings.sort_by_key(|o| std::cmp::Reverse(if white { o.as_white.games() } else { o.as_black.games() }));
        let rows: Vec<StackRow> = openings
            .into_iter()
            .take(6)
            .map(|o| {
                let tally = if white { &o.as_white } else { &o.as_black };
                stack_row(
                    opening_label(&o.name, o.eco.as_deref()),
                    plural(tally.games(), "game"),
                    tally,
                    &format!("{} as {}", o.name, if white { "White" } else { "Black" }),
                )
            })
            .collect();
        if rows.is_empty() {
            note(if white { "No classified games as White yet." } else { "No classified games as Black yet." }).upcast()
        } else {
            with_legend(&charts::stacked_bars(rows, if white { "Openings as White" } else { "Openings as Black" }))
        }
    };
    let white = card("Playing White", Some("Your openings with the white pieces"), &by_color(true));
    let black = card("Playing Black", Some("Your defenses with the black pieces"), &by_color(false));

    let trend = &stats.opening_trend;
    let trend_card = if trend.starts.len() >= 2 && !trend.series.is_empty() {
        let max = trend.series.iter().flat_map(|(_, counts)| counts.iter()).copied().max().unwrap_or(1).max(1);
        let y_max = (max.div_ceil(4) * 4) as f64;
        let tooltips = (0..trend.starts.len())
            .map(|index| {
                let mut text = format!("From {}", format_date(trend.starts[index]));
                for (name, counts) in &trend.series {
                    text.push_str(&format!("\n{name}: {}", plural(counts[index], "game")));
                }
                text
            })
            .collect();
        let chart = charts::line_chart(LineSpec {
            series: trend
                .series
                .iter()
                .enumerate()
                .map(|(i, (name, counts))| LineSeries {
                    name: name.clone(),
                    color: charts::SERIES[i % charts::SERIES.len()].into(),
                    values: counts.iter().map(|c| *c as f64).collect(),
                })
                .collect(),
            tooltips,
            x_labels: (0..trend.starts.len()).map(|i| (i, format_short_date(trend.starts[i]))).collect(),
            y_max,
            y_suffix: "",
            description: "Opening usage over time".into(),
        });
        let key = gtk::Label::new(None);
        key.set_wrap(true);
        key.set_xalign(0.0);
        key.add_css_class("caption");
        key.set_markup(
            &trend
                .series
                .iter()
                .enumerate()
                .map(|(i, (name, _))| {
                    format!(
                        "<span foreground=\"{}\">●</span> {}",
                        charts::SERIES[i % charts::SERIES.len()],
                        glib::markup_escape_text(name)
                    )
                })
                .collect::<Vec<_>>()
                .join("   "),
        );
        let column = gtk::Box::new(gtk::Orientation::Vertical, 6);
        column.append(&chart);
        column.append(&key);
        card("Opening trends over time", Some("Games per period for your five most played openings"), &column)
    } else {
        card("Opening trends over time", None, &note("Play a few more games to see how your repertoire changes."))
    };

    let eco = charts::hbar_chart(
        stats
            .eco_classes
            .iter()
            .enumerate()
            .map(|(index, tally)| HBar {
                label: ECO_NAMES[index].to_owned(),
                value: tally.games() as f64,
                color: charts::SERIES[index % charts::SERIES.len()].into(),
                value_text: tally.games().to_string(),
                tooltip: format!(
                    "{}\n{} · {}\nScore {}",
                    ECO_NAMES[index],
                    plural(tally.games(), "game"),
                    record_text(tally),
                    if tally.games() == 0 { "—".to_owned() } else { percent(tally.score_pct()) }
                ),
            })
            .collect(),
        "Games per ECO class",
    );
    let eco = card("ECO classes", Some("Which kinds of openings you play"), &eco);

    let mut children = vec![grid(vec![played, results], 2), grid(vec![best, worst], 2), grid(vec![white, black], 2), grid(vec![trend_card, eco], 2)];
    if stats.unclassified.games() > 0 {
        children.push(
            note(&format!(
                "{} not classified: they left the opening book on the first move or started from a custom position.",
                plural(stats.unclassified.games(), "game")
            ))
            .upcast(),
        );
    }
    page(children)
}

// -------------------------------------------------------- move quality ----

const HEADLINE_CLASSES: [MoveClass; 8] = [
    MoveClass::Brilliant,
    MoveClass::Great,
    MoveClass::Best,
    MoveClass::Excellent,
    MoveClass::Good,
    MoveClass::Inaccuracy,
    MoveClass::Mistake,
    MoveClass::Blunder,
];

fn quality_page(stats: &Stats) -> gtk::Widget {
    let quality = &stats.quality;
    if !quality.has_data() {
        let empty = adw::StatusPage::builder()
            .icon_name("kg-review-symbolic")
            .title("No engine-reviewed games yet")
            .description("Move quality comes only from real Stockfish analysis. Finish a game and press Game Review, or open a game in Game History and choose Review with Stockfish. Nothing is estimated.")
            .build();
        return empty.upcast();
    }
    let basis = note(&format!(
        "Based on {} of your {} games that were analyzed by Stockfish ({} of your moves).",
        quality.analyzed_games,
        quality.total_games,
        quality.total_moves
    ));

    let mut tiles = vec![tile(
        &quality.avg_accuracy.map_or("—".to_owned(), percent),
        "Average accuracy per game",
        None,
    )];
    for class in HEADLINE_CLASSES {
        let count = quality.counts[class.index()];
        let label = match class {
            MoveClass::Inaccuracy => "Inaccuracies".to_owned(),
            MoveClass::Mistake => "Mistakes".to_owned(),
            MoveClass::Blunder => "Blunders".to_owned(),
            other => format!("{} moves", other.label()),
        };
        let tile_widget = tile(&count.to_string(), &label, None);
        tile_widget.set_tooltip_text(Some(&format!(
            "{} ({:.1}% of your reviewed moves)",
            class.label(),
            MoveQuality::share(&quality.counts, quality.total_moves, class)
        )));
        tiles.push(tile_widget);
    }
    let tiles = grid(tiles, 5);

    let distribution = charts::hbar_chart(
        MoveClass::ALL
            .iter()
            .map(|class| {
                let count = quality.counts[class.index()];
                let share = MoveQuality::share(&quality.counts, quality.total_moves, *class);
                HBar {
                    label: class.label().to_owned(),
                    value: count as f64,
                    color: class.color().to_owned(),
                    value_text: format!("{count} · {share:.0}%"),
                    tooltip: format!("{}\n{count} moves ({share:.1}% of {})", class.label(), quality.total_moves),
                }
            })
            .collect(),
        "Move quality distribution",
    );
    let distribution = card("Move-quality distribution", Some("All of your reviewed moves"), &distribution);

    let heat = phase_heatmap(quality);
    let heat = card(
        "Quality by game phase",
        Some("Share of your moves in each class, per phase. Darker means more moves."),
        &heat,
    );

    let games = &quality.per_game;
    let trend = if games.len() >= 2 {
        let max = games
            .iter()
            .flat_map(|g| [g.inaccuracies_per_100(), g.mistakes_per_100(), g.blunders_per_100()])
            .fold(0.0, f64::max);
        let y_max = ((max / 4.0).ceil().max(1.0)) * 4.0;
        let tooltips = games
            .iter()
            .enumerate()
            .map(|(index, g)| {
                format!(
                    "Reviewed game {} · {} ({})\nInaccuracies: {:.1} per 100 moves\nMistakes: {:.1}\nBlunders: {:.1}\nAccuracy: {:.0}%",
                    index + 1,
                    format_date(g.played_at),
                    g.outcome.label(),
                    g.inaccuracies_per_100(),
                    g.mistakes_per_100(),
                    g.blunders_per_100(),
                    g.accuracy
                )
            })
            .collect();
        let series = |name: &str, color: &str, f: fn(&crate::history::stats::GameQuality) -> f64| LineSeries {
            name: name.to_owned(),
            color: color.to_owned(),
            values: games.iter().map(f).collect(),
        };
        let chart = charts::line_chart(LineSpec {
            series: vec![
                series("Inaccuracies", MoveClass::Inaccuracy.color(), |g| g.inaccuracies_per_100()),
                series("Mistakes", MoveClass::Mistake.color(), |g| g.mistakes_per_100()),
                series("Blunders", MoveClass::Blunder.color(), |g| g.blunders_per_100()),
            ],
            tooltips,
            x_labels: pick_labels(games.len()).into_iter().map(|i| (i, format_short_date(games[i].played_at))).collect(),
            y_max,
            y_suffix: "",
            description: "Mistakes and blunders over time".into(),
        });
        let key = gtk::Label::new(None);
        key.add_css_class("caption");
        key.set_xalign(0.0);
        key.set_markup(&format!(
            "<span foreground=\"{}\">●</span> Inaccuracies   <span foreground=\"{}\">●</span> Mistakes   <span foreground=\"{}\">●</span> Blunders   <span alpha=\"60%\">per 100 moves</span>",
            MoveClass::Inaccuracy.color(),
            MoveClass::Mistake.color(),
            MoveClass::Blunder.color()
        ));
        let column = gtk::Box::new(gtk::Orientation::Vertical, 6);
        column.append(&chart);
        column.append(&key);
        card("Mistakes and blunders over time", Some("Lower is better"), &column)
    } else {
        card("Mistakes and blunders over time", None, &note("Review at least two games to see the trend."))
    };

    let comparison = win_loss_comparison(quality);
    let accuracy_chart = if games.len() >= 2 {
        let tooltips = games
            .iter()
            .enumerate()
            .map(|(index, g)| format!("Reviewed game {} · {} ({})\nAccuracy {:.0}%", index + 1, format_date(g.played_at), g.outcome.label(), g.accuracy))
            .collect();
        card(
            "Accuracy per game",
            Some("Higher is better"),
            &charts::line_chart(LineSpec {
                series: vec![LineSeries {
                    name: "Accuracy".into(),
                    color: ACCENT.into(),
                    values: games.iter().map(|g| g.accuracy).collect(),
                }],
                tooltips,
                x_labels: pick_labels(games.len()).into_iter().map(|i| (i, format_short_date(games[i].played_at))).collect(),
                y_max: 100.0,
                y_suffix: "%",
                description: "Accuracy per reviewed game".into(),
            }),
        )
    } else {
        card("Accuracy per game", None, &note("Review at least two games to see the trend."))
    };

    page(vec![basis.upcast(), tiles, grid(vec![distribution, heat], 2), grid(vec![trend, accuracy_chart], 2), comparison])
}

fn phase_heatmap(quality: &MoveQuality) -> gtk::DrawingArea {
    let totals: Vec<usize> = (0..3).map(|phase| quality.heatmap[phase].iter().sum()).collect();
    let rows = MoveClass::ALL
        .iter()
        .map(|class| HeatRow { label: class.label().to_owned(), color: class.color().to_owned() })
        .collect();
    let cells = MoveClass::ALL
        .iter()
        .map(|class| {
            Phase::ALL
                .iter()
                .map(|phase| {
                    let count = quality.heatmap[phase.index()][class.index()];
                    let total = totals[phase.index()];
                    let share = if total == 0 { 0.0 } else { count as f64 * 100.0 / total as f64 };
                    HeatCell {
                        share,
                        text: if total == 0 { "–".into() } else { format!("{share:.0}%") },
                        tooltip: format!(
                            "{} moves in the {}\n{count} of {total} reviewed moves ({share:.1}%)",
                            class.label(),
                            phase.label().to_lowercase()
                        ),
                    }
                })
                .collect()
        })
        .collect();
    let columns = Phase::ALL.iter().map(|phase| format!("{} ({})", phase.label(), totals[phase.index()])).collect();
    charts::heatmap(rows, columns, cells, "Move quality by game phase")
}

fn win_loss_comparison(quality: &MoveQuality) -> gtk::Widget {
    let (win_counts, win_total) = &quality.in_wins;
    let (loss_counts, loss_total) = &quality.in_losses;
    if *win_total == 0 || *loss_total == 0 {
        return card(
            "Winning vs losing games",
            None,
            &note("Review at least one won and one lost game to compare your move quality in both."),
        );
    }
    let rows = MoveClass::ALL
        .iter()
        .map(|class| HeatRow { label: class.label().to_owned(), color: class.color().to_owned() })
        .collect();
    let cell = |counts: &[usize; CLASS_COUNT], total: usize, class: MoveClass, name: &str| {
        let count = counts[class.index()];
        let share = count as f64 * 100.0 / total as f64;
        HeatCell {
            share,
            text: format!("{share:.0}%"),
            tooltip: format!("{} moves in {name}\n{count} of {total} reviewed moves ({share:.1}%)", class.label()),
        }
    };
    let cells = MoveClass::ALL
        .iter()
        .map(|class| {
            vec![
                cell(win_counts, *win_total, *class, "won games"),
                cell(loss_counts, *loss_total, *class, "lost games"),
            ]
        })
        .collect();
    let chart = charts::heatmap(
        rows,
        vec![format!("Won games ({win_total})"), format!("Lost games ({loss_total})")],
        cells,
        "Move quality in won and lost games",
    );
    card(
        "Winning vs losing games",
        Some("How your move quality differs between games you won and games you lost"),
        &chart,
    )
}

// --------------------------------------------------- strengths/weaknesses ----

fn insight_row(insight: &Insight) -> gtk::Widget {
    let (css, icon) = match insight.kind {
        InsightKind::Strength => ("insight-strength", "emblem-ok-symbolic"),
        InsightKind::Weakness => ("insight-weakness", "dialog-warning-symbolic"),
        InsightKind::Improvement => ("insight-improvement", "go-up-symbolic"),
        InsightKind::Note => ("insight-note", "dialog-information-symbolic"),
    };
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row.add_css_class("insight");
    row.add_css_class(css);
    let image = gtk::Image::from_icon_name(icon);
    image.set_valign(gtk::Align::Start);
    image.set_pixel_size(20);
    image.add_css_class("insight-icon");
    let text = gtk::Box::new(gtk::Orientation::Vertical, 3);
    text.set_hexpand(true);
    let title = gtk::Label::new(Some(&insight.title));
    title.add_css_class("heading");
    title.set_xalign(0.0);
    title.set_wrap(true);
    let detail = gtk::Label::new(Some(&insight.detail));
    detail.set_xalign(0.0);
    detail.set_wrap(true);
    text.append(&title);
    text.append(&detail);
    if let Some(advice) = &insight.advice {
        let tip = gtk::Label::new(None);
        tip.set_markup(&format!("<b>Try this:</b> {}", glib::markup_escape_text(advice)));
        tip.set_xalign(0.0);
        tip.set_wrap(true);
        tip.add_css_class("dim-label");
        text.append(&tip);
    }
    row.append(&image);
    row.append(&text);
    row.upcast()
}

fn insight_group(title: &str, subtitle: &str, insights: &[&Insight]) -> Option<gtk::Widget> {
    if insights.is_empty() {
        return None;
    }
    let column = gtk::Box::new(gtk::Orientation::Vertical, 8);
    for insight in insights {
        column.append(&insight_row(insight));
    }
    Some(card(title, Some(subtitle), &column))
}

fn error_patterns_chart(errors: &ErrorPatterns) -> gtk::DrawingArea {
    let mut order: Vec<usize> = (0..6).collect();
    order.sort_by_key(|slot| std::cmp::Reverse(errors.by_piece[*slot]));
    charts::hbar_chart(
        order
            .into_iter()
            .map(|slot| {
                let count = errors.by_piece[slot];
                HBar {
                    label: PIECE_LABELS[slot].to_owned(),
                    value: count as f64,
                    color: MoveClass::Mistake.color().to_owned(),
                    value_text: count.to_string(),
                    tooltip: format!("{count} of {} mistakes and blunders moved a {}", errors.total, PIECE_LABELS[slot].to_lowercase()),
                }
            })
            .collect(),
        "Mistakes and blunders by moved piece",
    )
}

fn insights_page(stats: &Stats) -> gtk::Widget {
    let mut children: Vec<gtk::Widget> = Vec::new();

    // Radar + phase overview.
    let mut top: Vec<gtk::Widget> = Vec::new();
    if stats.radar.len() >= 3 {
        let radar = charts::radar(
            stats
                .radar
                .iter()
                .map(|axis| RadarPoint {
                    label: axis.label.to_owned(),
                    value: axis.value,
                    tooltip: format!("{}\n{}", axis.label, axis.detail),
                })
                .collect(),
            "Performance across areas of the game",
        );
        top.push(card("Performance radar", Some("0–100 in each area, from your own games"), &radar));
    } else {
        top.push(card(
            "Performance radar",
            None,
            &note("The radar needs results as both colors and engine-reviewed games. Keep playing and reviewing to fill it in."),
        ));
    }
    let phases = &stats.phases;
    let phase_rows = vec![
        stack_row("Reached an endgame".into(), plural(phases.endgame_reached.games(), "game"), &phases.endgame_reached, "Games that reached an endgame"),
        stack_row("Ended in the middlegame".into(), plural(phases.ended_middlegame.games(), "game"), &phases.ended_middlegame, "Games decided in the middlegame"),
        stack_row("Ended in the opening".into(), plural(phases.ended_opening.games(), "game"), &phases.ended_opening, "Games decided in the first 10 moves"),
    ];
    let phase_chart = with_legend(&charts::stacked_bars(phase_rows, "Results by the phase in which games ended"));
    let phase_card_content = gtk::Box::new(gtk::Orientation::Vertical, 10);
    phase_card_content.append(&phase_chart);
    let reviewed: Vec<_> = Phase::ALL.iter().filter(|p| phases.quality[p.index()].moves > 0).collect();
    if !reviewed.is_empty() {
        let errors = charts::hbar_chart(
            reviewed
                .iter()
                .map(|phase| {
                    let quality = &phases.quality[phase.index()];
                    let rate = quality.error_rate().unwrap_or(0.0);
                    HBar {
                        label: format!("{} errors", phase.label()),
                        value: rate,
                        color: MoveClass::Mistake.color().to_owned(),
                        value_text: format!("{rate:.0}%"),
                        tooltip: format!(
                            "{}\n{} of {} reviewed moves were inaccuracies or worse ({rate:.1}%)\n{} blunders · average accuracy {}",
                            phase.label(),
                            quality.errors,
                            quality.moves,
                            quality.blunders,
                            quality.avg_accuracy().map_or("—".to_owned(), percent)
                        ),
                    }
                })
                .collect(),
            "Share of reviewed moves that were errors, per phase",
        );
        let caption = gtk::Label::new(Some("Share of your reviewed moves that were inaccuracies, mistakes or blunders"));
        caption.add_css_class("caption");
        caption.add_css_class("dim-label");
        caption.set_xalign(0.0);
        caption.set_wrap(true);
        phase_card_content.append(&caption);
        phase_card_content.append(&errors);
    }
    top.push(card("Opening, middlegame and endgame", Some("Where your games are decided"), &phase_card_content));
    children.push(grid(top, 2));

    // Insights.
    let by_kind = |kind: InsightKind| stats.insights.iter().filter(|i| i.kind == kind).collect::<Vec<_>>();
    let groups = [
        insight_group("Strengths", "What is working for you", &by_kind(InsightKind::Strength)),
        insight_group("Weaknesses to work on", "Recurring problems and what to try", &by_kind(InsightKind::Weakness)),
        insight_group("Improvement", "Signs of progress", &by_kind(InsightKind::Improvement)),
        insight_group("Notes", "", &by_kind(InsightKind::Note)),
    ];
    let groups: Vec<gtk::Widget> = groups.into_iter().flatten().collect();
    if groups.is_empty() {
        children.push(card(
            "Insights",
            None,
            &note("Not enough games yet to spot patterns. Insights appear once you have a few games per opening, color or difficulty."),
        ));
    } else {
        children.extend(groups);
    }

    // Supporting detail.
    let mut detail: Vec<gtk::Widget> = Vec::new();
    if stats.errors.total > 0 {
        detail.push(card(
            "Which pieces your serious errors involve",
            Some(&format!(
                "{} mistakes and blunders · {} captures, {} quiet moves",
                stats.errors.total, stats.errors.captures, stats.errors.quiet
            )),
            &error_patterns_chart(&stats.errors),
        ));
    }
    if let Some(overall) = &stats.overall
        && !overall.loss_terminations.is_empty()
    {
        let max_losses = overall.tally.losses.max(1);
        let chart = charts::hbar_chart(
            overall
                .loss_terminations
                .iter()
                .map(|(name, count)| HBar {
                    label: name.clone(),
                    value: *count as f64,
                    color: LOSS.into(),
                    value_text: count.to_string(),
                    tooltip: format!("{name}: {} of your {} losses ({:.0}%)", count, overall.tally.losses, *count as f64 * 100.0 / max_losses as f64),
                })
                .collect(),
            "How your losses ended",
        );
        detail.push(card("How your losses ended", None, &chart));
    }
    if !detail.is_empty() {
        children.push(grid(detail, 2));
    }
    page(children)
}
