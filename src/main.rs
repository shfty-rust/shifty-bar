use notify::{recommended_watcher, Event, EventKind, Watcher};
use std::{
    collections::BTreeMap,
    error::Error,
    io::Read,
    path::Path,
    process::{Command, Stdio},
    time::Duration,
};

use chrono::prelude::*;

#[macro_export]
macro_rules ! cons {
    ($(,)?) => {
        ()
    };
    ($car:expr $(,)?) => {
        ($car, ())
    };
    ($car:expr, $($cdr:tt)*) => {
        ($car, $crate::cons!($($cdr)*))
    };
}

#[macro_export]
macro_rules ! Cons {
    ($(,)?) => {
        ()
    };
    ($car:ty $(,)?) => {
        ($car, ())
    };
    ($car:ty, $($cdr:tt)*) => {
        ($car, $crate::Cons!($($cdr)*))
    };
}

// Monitor names
const LEFT_MONITOR: &'static str = "DVI-D-0";
const RIGHT_MONITOR: &'static str = "DP-0";
const CENTER_MONITOR: &'static str = "DP-2";

// Basic colors
const WHITE: usize = 0xFFFFFFFF;
const BLACK: usize = 0xFF000000;
const TRANSPARENT: usize = 0x00000000;

// Dracula colors
const BACKGROUND: usize = 0xFF282a36;
const CURRENT_LINE: usize = 0xFF44475a;
const FOREGROUND: usize = 0xFFF8F8F2;
const COMMENT: usize = 0xFF6272A4;
const CYAN: usize = 0xFF8BE9FD;
const GREEN: usize = 0xFF50FA7B;
const ORANGE: usize = 0xFFFFB86C;
const PINK: usize = 0xFFFF79C6;
const PURPLE: usize = 0xFFBD93F9;
const RED: usize = 0xFFFF5555;
const YELLOW: usize = 0xFFF1FA8C;

type Monitors<'a> = Vec<&'a str>;
type MonitorIndices<'a> = BTreeMap<&'a str, usize>;
type MonitorDesktops<'a> = BTreeMap<&'a str, Vec<String>>;
type MonitorActiveDesktops<'a> = BTreeMap<String, String>;

// Application context
#[derive(Debug, Clone)]
struct Context<'a> {
    monitor_indices: MonitorIndices<'a>,
    monitor_desktops: MonitorDesktops<'a>,
    monitor_focused_desktops: MonitorActiveDesktops<'a>,
    focused_monitor: String,
    workspace: String,
    workspace_icon: Option<String>,
}

impl<'a> Context<'a> {
    pub fn new<S: Into<String>>(
        monitors: Monitors<'a>,
        monitor_desktops: MonitorDesktops<'a>,
        monitor_focused_desktops: MonitorActiveDesktops<'a>,
        focused_monitor: String,
        workspace: S,
        workspace_icon: Option<String>,
    ) -> Self {
        let monitor_indices = monitors
            .into_iter()
            .enumerate()
            .map(|(i, monitor)| (monitor, i))
            .collect();

        let workspace = workspace.into();
        Context {
            monitor_indices,
            monitor_desktops,
            monitor_focused_desktops,
            focused_monitor,
            workspace,
            workspace_icon,
        }
    }
}

// A type that can be rendered to a lemonbar-format string
trait Draw {
    fn draw(&self) -> String;
}

impl Draw for &str {
    fn draw(&self) -> String {
        self.to_string()
    }
}

impl Draw for String {
    fn draw(&self) -> String {
        self.to_string()
    }
}

impl<CAR, CDR> Draw for (CAR, CDR)
where
    CAR: Draw,
    CDR: Draw,
{
    fn draw(&self) -> String {
        self.0.draw() + &self.1.draw()
    }
}

impl Draw for () {
    fn draw(&self) -> String {
        Default::default()
    }
}

fn read_workspace() -> String {
    let mut string = std::fs::read_to_string("/home/josh/.local/state/workspace").unwrap();
    // Strip EOF character
    string.pop().unwrap();
    string
}

fn bspc_query(args: &str) -> Result<String, Box<dyn Error>> {
    let mut command = Command::new("bspc");
    command.arg("query");
    for a in args.split(" ") {
        command.arg(a);
    }

    let result = command.output()?;
    if result.stderr.len() > 0 {
        panic!(
            "Failed to fetch desktop information: {}",
            String::from_utf8(result.stderr).unwrap()
        );
    }

    let mut result = String::from_utf8(result.stdout)?;
    result
        .pop()
        .expect("Result string not terminated with newline");

    Ok(result)
}

fn main() {
    // Read workspace files
    let workflows = workflow::Workflows::new("/home/josh/.config/workflow").unwrap();

    // Fetch monitor information from xrandr
    let monitors = Command::new("sh")
        .arg("-c")
        .arg("xrandr | grep ' connected ' | sed 's/ connected .*$//'")
        .output()
        .unwrap();

    if monitors.stderr.len() > 0 {
        panic!(
            "Failed to fetch monitor information: {}",
            String::from_utf8(monitors.stderr).unwrap()
        );
    }

    let monitors = String::from_utf8(monitors.stdout).unwrap();
    let monitors = monitors
        .split('\n')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();

    // Subscrbe to bspc
    let (tx_monitor_focus, rx_monitor_focus) = crossbeam_channel::unbounded();
    let (tx_desktop_focus, rx_desktop_focus) = crossbeam_channel::unbounded();
    std::thread::spawn(move || {
        let desktop_focus = Command::new("zsh")
            .arg("-c")
            .arg("bspc subscribe desktop_focus monitor_focus")
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();

        let mut focus_stdout = desktop_focus.stdout.unwrap();

        let mut buf = [0; 256];
        loop {
            if let Ok(bytes) = focus_stdout.read(&mut buf) {
                let string = String::from_utf8(buf[0..bytes].to_vec()).unwrap();
                let strings = string
                    .split('\n')
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>();

                for string in strings {
                    let string = string
                        .split(' ')
                        .map(ToString::to_string)
                        .collect::<Vec<_>>();

                    match string[0].as_str() {
                        "desktop_focus" => {
                            let monitor_name =
                                bspc_query(&format!("-M -m {} --names", &string[1])).unwrap();
                            let desktop_name =
                                bspc_query(&format!("-D -d {} --names", &string[2])).unwrap();
                            tx_desktop_focus.send((monitor_name, desktop_name)).unwrap();
                        }
                        "monitor_focus" => {
                            let monitor_name =
                                bspc_query(&format!("-M -m {} --names", &string[1])).unwrap();
                            tx_monitor_focus.send(monitor_name).unwrap();
                        }
                        _ => (),
                    }
                }
            }
        }
    });

    // Create a channel for notifying the main thread of changes to the workspace file
    let (tx_workspace, rx_workspace) = crossbeam_channel::unbounded();

    // Create a watcher to monitor the workspace file
    let mut watcher = recommended_watcher(move |res| match res {
        Ok(Event { kind, .. }) => match kind {
            EventKind::Modify(_) => tx_workspace.send(read_workspace()).unwrap(),
            _ => (),
        },
        Err(e) => panic!("watch error: {:?}", e),
    })
    .unwrap();

    // Fetch desktop information from bspc
    let (desktops, focused_desktops) = monitors
        .iter()
        .map(|monitor| {
            let desktops = bspc_query(&format!("-D -m {} --names", monitor)).unwrap();

            let desktops = desktops
                .split('\n')
                .filter(|s| !s.is_empty())
                .map(ToString::to_string)
                .collect::<Vec<_>>();

            let focused_desktop =
                bspc_query(&format!("-D -d {}:focused --names", monitor)).unwrap();

            ((*monitor, desktops), (monitor.to_string(), focused_desktop))
        })
        .unzip();

    let focused_monitor = bspc_query("-M -m focused --names").unwrap();

    // Start the watcher
    watcher
        .watch(
            Path::new("/home/josh/.local/state/workspace"),
            notify::RecursiveMode::NonRecursive,
        )
        .unwrap();

    // Create bar context
    let workspace = read_workspace();
    let workspace_icon = if let Some(workflow) = workflows.workflow(&workspace) {
        workflow.icon.clone()
    } else {
        None
    };

    let mut context = Context::new(
        monitors,
        desktops,
        focused_desktops,
        focused_monitor,
        workspace,
        workspace_icon,
    );

    // Create a tick receiver to handle continual updates
    let rx_tick = crossbeam_channel::tick(Duration::from_secs(1));

    // Enter main loop
    loop {
        crossbeam_channel::select! {
            // Received a tick event, do nothing
            recv(rx_tick) -> _ => (),
            // Received a workspace change event, update context
            recv(rx_workspace) -> workspace => {
                let workspace = workspace.unwrap();

                let workspace_path = std::fs::read_to_string("/home/josh/.local/state/workspace")
                    .unwrap()
                    .replace("\n", "");

                if let Some(workflow) = workflows.workflow(&workspace_path) {
                    context.workspace_icon = workflow.icon.clone();
                }

                context.workspace = workspace;
            }
            // Received a desktop focus event, update context
            recv(rx_desktop_focus) -> desktop_focus => {
                let (monitor, desktop) = desktop_focus.unwrap();
                context.monitor_focused_desktops.insert(monitor, desktop.to_string());
            }
            // Received a monitor focus event, update context
            recv(rx_monitor_focus) -> monitor_focus => {
                context.focused_monitor = monitor_focus.unwrap();
            }
        };

        println!("{}", widget_bar(&mut context).draw());
    }
}

// Lemonbar elements
const fn lemonbar_align_left() -> &'static str {
    "%{l}"
}

const fn lemonbar_align_center() -> &'static str {
    "%{c}"
}

const fn lemonbar_align_right() -> &'static str {
    "%{r}"
}

const fn lemonbar_foreground_reset() -> &'static str {
    "%{F-}"
}

const fn lemonbar_background_reset() -> &'static str {
    "%{B-}"
}

fn lemonbar_color_reset() -> String {
    format!(
        "{}{}",
        lemonbar_foreground_reset(),
        lemonbar_background_reset()
    )
}

fn lemonbar_foreground(color: usize) -> String {
    format!("%{{F#{:08X}}}", color)
}

fn lemonbar_background(color: usize) -> String {
    format!("%{{B#{:08X}}}", color)
}

fn lemonbar_color(foreground: usize, background: usize) -> String {
    format!(
        "{}{}",
        lemonbar_foreground(foreground),
        lemonbar_background(background)
    )
}

fn lemonbar_monitor(index: usize) -> String {
    format!("%{{S{}}}", index.to_string())
}

// Special characters
const fn char_right_arrow() -> &'static str {
    ""
}

const fn char_left_arrow() -> &'static str {
    ""
}

const fn char_left_angle() -> &'static str {
    ""
}

const fn char_right_angle() -> &'static str {
    ""
}

const fn char_folder() -> &'static str {
    ""
}

const fn char_space() -> &'static str {
    " "
}

const fn char_clock() -> &'static str {
    ""
}

// Widgets
fn widget_on_monitor(index: usize, d: impl Draw) -> impl Draw {
    cons![lemonbar_monitor(index), d]
}

fn widget_repeat(count: usize, d: impl Draw + Clone) -> impl Draw {
    std::iter::repeat(d)
        .take(count)
        .map(|d| d.draw())
        .collect::<String>()
}

fn widget_conditional(condition: bool, d: impl Draw) -> impl Draw {
    if condition {
        d.draw()
    } else {
        String::default()
    }
}

fn widget_padded(count: usize, character: impl Draw + Clone, d: impl Draw) -> impl Draw {
    cons![
        widget_repeat(count, character.clone()),
        d,
        widget_repeat(count, character),
    ]
}

fn widget_pad_whitespace(count: usize, d: impl Draw) -> impl Draw {
    widget_padded(count, " ", d)
}

fn widget_align_left(d: impl Draw) -> impl Draw {
    cons![lemonbar_align_left(), d]
}

fn widget_align_center(d: impl Draw) -> impl Draw {
    cons![lemonbar_align_center(), d]
}

fn widget_align_right(d: impl Draw) -> impl Draw {
    cons![lemonbar_align_right(), d]
}

fn widget_colored(foreground: usize, background: usize, d: impl Draw) -> impl Draw {
    cons![lemonbar_color(foreground, background), d]
}

fn widget_right_panel(
    foreground: usize,
    background: usize,
    cap: impl Draw,
    d: impl Draw,
) -> impl Draw {
    cons![
        widget_colored(background, TRANSPARENT, cap),
        widget_colored(foreground, background, widget_pad_whitespace(1, d)),
        lemonbar_color_reset(),
    ]
}

fn widget_center_panel(
    foreground: usize,
    background: usize,
    left_cap: impl Draw,
    right_cap: impl Draw,
    d: impl Draw,
) -> impl Draw {
    cons![
        widget_colored(background, TRANSPARENT, left_cap),
        widget_colored(foreground, background, widget_pad_whitespace(1, d)),
        widget_colored(background, TRANSPARENT, right_cap),
        lemonbar_color_reset(),
    ]
}

fn widget_left_panel(
    foreground: usize,
    background: usize,
    right_cap: impl Draw,
    d: impl Draw,
) -> impl Draw {
    cons![
        widget_colored(foreground, background, widget_padded(1, " ", d)),
        widget_colored(background, TRANSPARENT, right_cap),
        lemonbar_color_reset(),
    ]
}

fn widget_left_arrow_panel(foreground: usize, background: usize, d: impl Draw) -> impl Draw {
    widget_left_panel(foreground, background, char_right_arrow(), d)
}

fn widget_right_arrow_panel(foreground: usize, background: usize, d: impl Draw) -> impl Draw {
    widget_right_panel(foreground, background, char_left_arrow(), d)
}

fn widget_angle_center_panel(foreground: usize, background: usize, d: impl Draw) -> impl Draw {
    widget_center_panel(
        foreground,
        background,
        char_left_angle(),
        char_right_angle(),
        d,
    )
}

fn widget_time(f: &str) -> impl Draw {
    Local::now().format(f).to_string()
}

fn widget_hostname() -> impl Draw {
    hostname::get()
        .expect("Failed to get hostname")
        .into_string()
        .expect("Failed to convert hostname to String")
}

fn widget_desktops<'a>(context: &Context<'a>, monitor: &'a str) -> impl Draw {
    let desktops = &context.monitor_desktops[&monitor];
    let focused_desktop = &context.monitor_focused_desktops[monitor];

    let mut out = String::default();
    for desktop in desktops.iter() {
        out += if desktop == focused_desktop { "[" } else { " " };

        out += desktop;

        out += if desktop == focused_desktop { "]" } else { " " };
    }
    out
}

fn widget_clock_panel() -> impl Draw {
    widget_right_arrow_panel(
        FOREGROUND,
        CURRENT_LINE,
        cons![widget_time("%D %H:%M:%S %p"), char_space(), char_clock(),],
    )
}

fn widget_center_bar(context: &Context) -> impl Draw {
    widget_align_center(widget_angle_center_panel(
        FOREGROUND,
        CURRENT_LINE,
        cons![
            context
                .workspace_icon
                .clone()
                .unwrap_or(char_folder().to_string()),
            context.workspace.clone(),
        ],
    ))
}

fn widget_bar(context: &mut Context) -> impl Draw {
    let center_index = context.monitor_indices[CENTER_MONITOR];
    let left_index = context.monitor_indices[LEFT_MONITOR];
    let right_index = context.monitor_indices[RIGHT_MONITOR];

    // TODO: Figure out how lemonbar determines monitor order
    cons![
        widget_on_monitor(
            right_index,
            cons![
                widget_align_left(widget_left_arrow_panel(
                    FOREGROUND,
                    CURRENT_LINE,
                    widget_desktops(context, CENTER_MONITOR),
                )),
                widget_conditional(
                    context.focused_monitor == CENTER_MONITOR,
                    widget_center_bar(context)
                ),
                widget_align_right(widget_clock_panel()),
            ]
        ),
        widget_on_monitor(
            left_index,
            cons![
                widget_align_left(widget_left_arrow_panel(
                    FOREGROUND,
                    CURRENT_LINE,
                    widget_desktops(context, LEFT_MONITOR),
                )),
                widget_conditional(
                    context.focused_monitor == LEFT_MONITOR,
                    widget_center_bar(context)
                ),
            ]
        ),
        widget_on_monitor(
            center_index,
            cons![
                widget_conditional(
                    context.focused_monitor == RIGHT_MONITOR,
                    widget_center_bar(context)
                ),
                widget_align_right(widget_right_arrow_panel(
                    FOREGROUND,
                    CURRENT_LINE,
                    widget_desktops(context, RIGHT_MONITOR),
                ))
            ]
        ),
    ]
}
