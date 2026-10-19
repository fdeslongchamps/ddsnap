//! Main window: welcome page, outputs, layer tree, GPU preview, export, progress and log.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::time::Duration;

use adw::prelude::*;
use ddsnap::core::dds::{self, Format, Size};
use ddsnap::core::diagnostics;
use ddsnap::core::export::{self, Report, Settings};
use ddsnap::core::helper::HelperError;
use ddsnap::core::layers::LayerTree;
use ddsnap::core::recipe::{self, Recipe};
use gtk::{gdk, gio, glib, graphene, gsk, pango};

use super::APP_ID;
use super::preview::{self, LayerTexture, View};
use super::worker::{self, Msg, Session};

const FORMATS: [Format; 5] = [
    Format::Auto,
    Format::Bc1,
    Format::Bc3,
    Format::Bc7,
    Format::Rgba8,
];
const FORMAT_LABELS: [&str; 5] = [
    "Auto",
    "BC1 (DXT1)",
    "BC3 (DXT5)",
    "BC7",
    "RGBA8 (non compressé)",
];
const SIZES: [Size; 5] = [
    Size::Source,
    Size::Max(4096),
    Size::Max(2048),
    Size::Max(1024),
    Size::Max(512),
];
const SIZE_LABELS: [&str; 5] = ["Source", "4096 max", "2048 max", "1024 max", "512 max"];
/// Longest side of the preview canvas, in pixels (ADR-005).
const PREVIEW_SIDE: f64 = 2048.0;

type Task = Box<dyn FnOnce(&Rc<Window>)>;

#[derive(Default)]
struct State {
    psd: Option<PathBuf>,
    tree: Option<LayerTree>,
    recipe: Option<Recipe>,
    /// False when the recipe on disk could not be read: never overwrite it.
    recipe_saves: bool,
    recipe_problem: Option<String>,
    current: usize,
    /// By layer index.
    checked: Vec<bool>,
    scale: f64,
    /// Extracted layers; `None` = empty or refused by the helper (not retried).
    textures: HashMap<usize, Option<LayerTexture>>,
    composite: Option<gdk::Texture>,
    compressed: Option<gdk::Texture>,
    compress_gen: u64,
    /// `None` while a job holds the helper.
    session: Option<Session>,
    busy: bool,
    queue: VecDeque<Task>,
    cancel: Arc<AtomicBool>,
    no_preview: bool,
    /// The user stopped the extraction, or it failed: do not restart it on its own.
    preview_stopped: bool,
}

/// One output ready to export, captured when the user clicks.
struct ExportJob {
    file: String,
    visible: Vec<usize>,
    as_saved: bool,
    settings: Settings,
    dest: PathBuf,
    warnings: Vec<String>,
}

pub struct Window {
    window: adw::ApplicationWindow,
    title: adw::WindowTitle,
    toasts: adw::ToastOverlay,
    pages: gtk::Stack,
    banner: adw::Banner,
    outputs: gtk::ListBox,
    format: adw::ComboRow,
    size: adw::ComboRow,
    mipmaps: adw::SwitchRow,
    out_dir: adw::ActionRow,
    out_dir_reset: gtk::Button,
    layers: gtk::ListBox,
    checks: RefCell<Vec<gtk::CheckButton>>,
    preview_pages: gtk::Stack,
    picture: gtk::Picture,
    scroller: gtk::ScrolledWindow,
    view: gtk::DropDown,
    actual_size: gtk::ToggleButton,
    compressed: gtk::ToggleButton,
    progress_bar: gtk::Revealer,
    progress: gtk::ProgressBar,
    stop: gtk::Button,
    log: gtk::TextView,
    state: RefCell<State>,
    /// Set while the code itself changes widgets, so their handlers do nothing.
    updating: Cell<bool>,
    pulsing: Cell<bool>,
}

impl Window {
    pub fn new(app: &adw::Application) -> Rc<Window> {
        let title = adw::WindowTitle::new("Ddsnap", "");
        let header = adw::HeaderBar::builder().title_widget(&title).build();
        let open = gtk::Button::builder()
            .icon_name("document-open-symbolic")
            .tooltip_text("Ouvrir un PSD (Ctrl+O)")
            .action_name("win.open")
            .build();
        header.pack_start(&open);

        let export_menu = gio::Menu::new();
        export_menu.append(Some("Tout exporter"), Some("win.export-all"));
        let export = adw::SplitButton::builder()
            .label("Exporter")
            .tooltip_text("Exporter la sortie choisie (Ctrl+E) ; Tout exporter : Ctrl+Maj+E")
            .menu_model(&export_menu)
            .action_name("win.export")
            .build();
        export.add_css_class("suggested-action");
        let main_menu = gio::Menu::new();
        main_menu.append(Some("Sans aperçu"), Some("win.no-preview"));
        main_menu.append(Some("Diagnostic"), Some("win.diagnostic"));
        main_menu.append(Some("À propos de Ddsnap"), Some("win.about"));
        let menu = gtk::MenuButton::builder()
            .icon_name("open-menu-symbolic")
            .menu_model(&main_menu)
            .tooltip_text("Menu principal")
            .build();
        let show_log = gtk::ToggleButton::builder()
            .icon_name("utilities-terminal-symbolic")
            .tooltip_text("Journal")
            .build();
        header.pack_end(&menu);
        header.pack_end(&show_log);
        header.pack_end(&export);

        // Welcome page.
        let open_big = gtk::Button::builder()
            .label("Ouvrir un PSD…")
            .action_name("win.open")
            .halign(gtk::Align::Center)
            .build();
        open_big.add_css_class("pill");
        open_big.add_css_class("suggested-action");
        let welcome = adw::StatusPage::builder()
            .icon_name("image-x-generic-symbolic")
            .title("Déposez un PSD")
            .description("ou ouvrez-en un avec Ctrl+O. Les DDS sont écrits à côté du PSD, dans le dossier de la skin.")
            .child(&open_big)
            .build();

        // Sidebar: outputs.
        let outputs = gtk::ListBox::new();
        outputs.add_css_class("boxed-list");
        let output_buttons = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        output_buttons.add_css_class("linked");
        let add = icon_button("list-add-symbolic", "Ajouter une sortie");
        let duplicate = icon_button("edit-copy-symbolic", "Dupliquer la sortie");
        let rename = icon_button("document-edit-symbolic", "Renommer la sortie");
        let delete = icon_button("user-trash-symbolic", "Supprimer la sortie");
        for b in [&add, &duplicate, &rename, &delete] {
            output_buttons.append(b);
        }
        let outputs_group = adw::PreferencesGroup::builder()
            .title("Sorties")
            .header_suffix(&output_buttons)
            .build();
        outputs_group.add(&outputs);

        let format = adw::ComboRow::builder()
            .title("Format")
            .model(&gtk::StringList::new(&FORMAT_LABELS))
            .build();
        let size = adw::ComboRow::builder()
            .title("Taille")
            .model(&gtk::StringList::new(&SIZE_LABELS))
            .build();
        let mipmaps = adw::SwitchRow::builder().title("Mipmaps").build();
        let choose_dir = gtk::Button::builder()
            .label("Choisir…")
            .valign(gtk::Align::Center)
            .build();
        let out_dir_reset = icon_button("edit-undo-symbolic", "Revenir au dossier du PSD");
        out_dir_reset.set_valign(gtk::Align::Center);
        let out_dir = adw::ActionRow::builder()
            .title("Dossier de sortie")
            .subtitle_lines(2)
            .build();
        out_dir.add_suffix(&out_dir_reset);
        out_dir.add_suffix(&choose_dir);
        let settings_group = adw::PreferencesGroup::builder()
            .title("Sortie choisie")
            .build();
        settings_group.add(&format);
        settings_group.add(&size);
        settings_group.add(&mipmaps);
        settings_group.add(&out_dir);

        let layers = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .build();
        layers.add_css_class("boxed-list");
        let layers_group = adw::PreferencesGroup::builder()
            .title("Calques")
            .description("Cochés = dessinés dans la sortie choisie")
            .build();
        layers_group.add(&layers);

        let sidebar_box = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(18)
            .margin_top(12)
            .margin_bottom(12)
            .margin_start(12)
            .margin_end(12)
            .build();
        sidebar_box.append(&outputs_group);
        sidebar_box.append(&settings_group);
        sidebar_box.append(&layers_group);
        let sidebar = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .child(&sidebar_box)
            .width_request(340)
            .build();

        // Preview.
        let view = gtk::DropDown::from_strings(&View::LABELS);
        view.set_tooltip_text(Some("Canaux affichés"));
        let actual_size = gtk::ToggleButton::builder()
            .icon_name("zoom-original-symbolic")
            .tooltip_text("Taille réelle de l'aperçu (sinon ajusté à la fenêtre)")
            .build();
        let compressed = gtk::ToggleButton::builder()
            .label("Aperçu compressé")
            .tooltip_text("Montre le résultat après compression DDS")
            .build();
        let preview_tools = gtk::Box::builder()
            .spacing(6)
            .margin_top(6)
            .margin_bottom(6)
            .margin_start(6)
            .margin_end(6)
            .build();
        preview_tools.append(&view);
        preview_tools.append(&actual_size);
        preview_tools.append(&compressed);
        let picture = gtk::Picture::builder()
            .content_fit(gtk::ContentFit::Contain)
            .build();
        let scroller = gtk::ScrolledWindow::builder()
            .child(&picture)
            .vexpand(true)
            .hexpand(true)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Never)
            .build();
        let preview_off = adw::StatusPage::builder()
            .icon_name("image-missing-symbolic")
            .title("Aperçu désactivé")
            .description("Menu › Sans aperçu. L'export fonctionne sans.")
            .build();
        let preview_pages = gtk::Stack::new();
        preview_pages.add_named(&scroller, Some("picture"));
        preview_pages.add_named(&preview_off, Some("off"));
        let preview_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        preview_box.append(&preview_tools);
        preview_box.append(&preview_pages);

        let paned = gtk::Paned::builder()
            .start_child(&sidebar)
            .end_child(&preview_box)
            .shrink_start_child(false)
            .resize_start_child(false)
            .vexpand(true)
            .build();
        let banner = adw::Banner::new("");
        let main_page = gtk::Box::new(gtk::Orientation::Vertical, 0);
        main_page.append(&banner);
        main_page.append(&paned);

        let pages = gtk::Stack::new();
        pages.add_named(&welcome, Some("welcome"));
        pages.add_named(&main_page, Some("main"));

        // Bottom: progress and log.
        let progress = gtk::ProgressBar::builder()
            .show_text(true)
            .hexpand(true)
            .valign(gtk::Align::Center)
            .build();
        let stop = gtk::Button::builder()
            .label("Arrêter l'aperçu")
            .tooltip_text("Arrête l'extraction des calques ; l'export reste possible")
            .build();
        let progress_box = gtk::Box::builder()
            .spacing(12)
            .margin_top(6)
            .margin_bottom(6)
            .margin_start(12)
            .margin_end(12)
            .build();
        progress_box.append(&progress);
        progress_box.append(&stop);
        let progress_bar = gtk::Revealer::builder().child(&progress_box).build();
        let log = gtk::TextView::builder()
            .editable(false)
            .monospace(true)
            .wrap_mode(gtk::WrapMode::WordChar)
            .left_margin(12)
            .right_margin(12)
            .build();
        let log_scroller = gtk::ScrolledWindow::builder()
            .child(&log)
            .height_request(160)
            .build();
        let log_revealer = gtk::Revealer::builder().child(&log_scroller).build();
        show_log
            .bind_property("active", &log_revealer, "reveal-child")
            .sync_create()
            .build();
        let bottom = gtk::Box::new(gtk::Orientation::Vertical, 0);
        bottom.append(&progress_bar);
        bottom.append(&log_revealer);

        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&header);
        toolbar.set_content(Some(&pages));
        toolbar.add_bottom_bar(&bottom);
        let toasts = adw::ToastOverlay::new();
        toasts.set_child(Some(&toolbar));
        let window = adw::ApplicationWindow::builder()
            .application(app)
            .title("Ddsnap")
            .default_width(1280)
            .default_height(820)
            .content(&toasts)
            .build();

        let this = Rc::new(Window {
            window,
            title,
            toasts,
            pages,
            banner,
            outputs,
            format,
            size,
            mipmaps,
            out_dir,
            out_dir_reset,
            layers,
            checks: RefCell::default(),
            preview_pages,
            picture,
            scroller,
            view,
            actual_size,
            compressed,
            progress_bar,
            progress,
            stop,
            log,
            state: RefCell::new(State {
                scale: 1.0,
                ..State::default()
            }),
            updating: Cell::new(false),
            pulsing: Cell::new(false),
        });
        this.connect(&add, &duplicate, &rename, &delete, &choose_dir);
        // Handlers hold weak references: this one keeps the window state alive until close.
        let keep = RefCell::new(Some(this.clone()));
        this.window.connect_close_request(move |_| {
            keep.take();
            glib::Propagation::Proceed
        });
        this.set_has_psd(false);
        this
    }

    pub fn present(&self) {
        self.window.present();
    }

    fn connect(
        self: &Rc<Self>,
        add: &gtk::Button,
        duplicate: &gtk::Button,
        rename: &gtk::Button,
        delete: &gtk::Button,
        choose_dir: &gtk::Button,
    ) {
        self.action("open", |this| this.choose_psd());
        self.action("export", |this| this.export(false));
        self.action("export-all", |this| this.export(true));
        self.action("about", |this| this.about());
        self.action("diagnostic", |this| this.diagnostic());
        let no_preview = gio::SimpleAction::new_stateful("no-preview", None, &false.to_variant());
        let this = Rc::downgrade(self);
        no_preview.connect_activate(move |action, _| {
            let on = !action
                .state()
                .and_then(|s| s.get::<bool>())
                .unwrap_or(false);
            action.set_state(&on.to_variant());
            if let Some(this) = this.upgrade() {
                this.set_no_preview(on);
            }
        });
        self.window.add_action(&no_preview);

        let drop = gtk::DropTarget::new(gio::File::static_type(), gdk::DragAction::COPY);
        let this = Rc::downgrade(self);
        drop.connect_drop(move |_, value, _, _| {
            let path = value.get::<gio::File>().ok().and_then(|f| f.path());
            match (path, this.upgrade()) {
                (Some(path), Some(this)) => {
                    this.open(path);
                    true
                }
                _ => false,
            }
        });
        self.window.add_controller(drop);

        self.on_click(add, |this| this.add_output(false));
        self.on_click(duplicate, |this| this.add_output(true));
        self.on_click(rename, |this| this.rename_output());
        self.on_click(delete, |this| this.delete_output());
        self.on_click(choose_dir, |this| this.choose_out_dir());
        self.on_click(&self.out_dir_reset, |this| this.set_out_dir(None));
        self.on_click(&self.stop, |this| {
            let mut s = this.state.borrow_mut();
            s.preview_stopped = true;
            s.cancel.store(true, Ordering::Relaxed);
        });

        let this = Rc::downgrade(self);
        self.outputs.connect_row_selected(move |_, row| {
            if let (Some(this), Some(row)) = (this.upgrade(), row)
                && !this.updating.get()
            {
                this.select_output(row.index() as usize);
            }
        });
        let this = Rc::downgrade(self);
        self.format.connect_selected_notify(move |row| {
            let format = FORMATS[(row.selected() as usize).min(FORMATS.len() - 1)];
            if let Some(this) = this.upgrade() {
                this.edit_output(|o| o.format = format);
            }
        });
        let this = Rc::downgrade(self);
        self.size.connect_selected_notify(move |row| {
            let size = SIZES[(row.selected() as usize).min(SIZES.len() - 1)];
            if let Some(this) = this.upgrade() {
                this.edit_output(|o| o.size = size);
            }
        });
        let this = Rc::downgrade(self);
        self.mipmaps.connect_active_notify(move |row| {
            let on = row.is_active();
            if let Some(this) = this.upgrade() {
                this.edit_output(|o| o.mipmaps = on);
            }
        });

        let this = Rc::downgrade(self);
        self.view.connect_selected_notify(move |_| {
            if let Some(this) = this.upgrade() {
                this.show();
            }
        });
        let this = Rc::downgrade(self);
        self.actual_size.connect_toggled(move |button| {
            let actual = button.is_active();
            let Some(this) = this.upgrade() else { return };
            let policy = if actual {
                gtk::PolicyType::Automatic
            } else {
                gtk::PolicyType::Never
            };
            this.scroller.set_policy(policy, policy);
            this.picture.set_can_shrink(!actual);
        });
        let this = Rc::downgrade(self);
        self.compressed.connect_toggled(move |_| {
            if let Some(this) = this.upgrade() {
                this.refresh_compressed();
            }
        });
    }

    fn action(self: &Rc<Self>, name: &str, f: impl Fn(&Rc<Self>) + 'static) {
        let action = gio::SimpleAction::new(name, None);
        let this = Rc::downgrade(self);
        action.connect_activate(move |_, _| {
            if let Some(this) = this.upgrade() {
                f(&this);
            }
        });
        self.window.add_action(&action);
    }

    fn on_click(self: &Rc<Self>, button: &gtk::Button, f: impl Fn(&Rc<Self>) + 'static) {
        let this = Rc::downgrade(self);
        button.connect_clicked(move |_| {
            if let Some(this) = this.upgrade() {
                f(&this);
            }
        });
    }

    fn set_has_psd(&self, has: bool) {
        for name in ["export", "export-all"] {
            if let Some(action) = self.window.lookup_action(name) {
                action
                    .downcast::<gio::SimpleAction>()
                    .expect("our own action")
                    .set_enabled(has);
            }
        }
    }

    // ── Jobs ───────────────────────────────────────────────────────────────

    /// Runs `task` now, or after the running job. A running preview extraction is cut short
    /// (it resumes afterwards); an export always finishes.
    fn start(self: &Rc<Self>, task: impl FnOnce(&Rc<Self>) + 'static) {
        let busy = self.state.borrow().busy;
        if busy {
            let mut s = self.state.borrow_mut();
            s.cancel.store(true, Ordering::Relaxed);
            s.queue.push_back(Box::new(task));
            drop(s);
            self.log("en attente de la fin de la tâche en cours");
        } else {
            task(self);
        }
    }

    /// Runs `work` with the helper on a thread. Must be called from `start` (or `next`).
    fn run_job<T: Send + 'static>(
        self: &Rc<Self>,
        stoppable: bool,
        work: impl FnOnce(&mut Session, &Sender<Msg>, &AtomicBool) -> Result<T, String> + Send + 'static,
        done: impl FnOnce(&Rc<Self>, Result<T, String>) + 'static,
    ) {
        let (session, cancel) = {
            let mut s = self.state.borrow_mut();
            s.busy = true;
            s.cancel.store(false, Ordering::Relaxed);
            (s.session.take(), s.cancel.clone())
        };
        self.set_busy(true, stoppable);
        let on_msg = Rc::downgrade(self);
        let this = self.clone();
        worker::background(
            move |tx| {
                let mut session = match session {
                    Some(session) => session,
                    None => {
                        let _ = tx.send(Msg::Step("démarrage de l'assistant psd-tools".into()));
                        match Session::spawn() {
                            Ok(session) => session,
                            Err(e) => return (None, Err(e.to_string())),
                        }
                    }
                };
                let result = work(&mut session, tx, &cancel);
                // A dead helper is dropped; the next job starts a new one.
                let alive = session.helper.is_alive();
                (alive.then_some(session), result)
            },
            move |msg| {
                if let Some(this) = on_msg.upgrade() {
                    this.on_msg(msg);
                }
            },
            move |(session, result)| {
                {
                    let mut s = this.state.borrow_mut();
                    s.session = session;
                    s.busy = false;
                }
                this.set_busy(false, false);
                done(&this, result);
                this.next();
            },
        );
    }

    fn next(self: &Rc<Self>) {
        let task = self.state.borrow_mut().queue.pop_front();
        match task {
            Some(task) => task(self),
            None => self.maybe_extract(),
        }
    }

    fn set_busy(self: &Rc<Self>, busy: bool, stoppable: bool) {
        self.progress_bar.set_reveal_child(busy);
        self.stop.set_visible(stoppable);
        self.progress.set_fraction(0.0);
        self.pulsing.set(busy);
        if busy {
            let this = Rc::downgrade(self);
            glib::timeout_add_local(Duration::from_millis(150), move || match this.upgrade() {
                Some(this) if this.state.borrow().busy => {
                    if this.pulsing.get() {
                        this.progress.pulse();
                    }
                    glib::ControlFlow::Continue
                }
                _ => glib::ControlFlow::Break,
            });
        }
    }

    fn on_msg(self: &Rc<Self>, msg: Msg) {
        match msg {
            Msg::Step(step) => {
                self.progress.set_text(Some(&step));
                self.log(&step);
            }
            Msg::Fraction(f) => {
                self.pulsing.set(false);
                self.progress.set_fraction(f);
            }
            Msg::Layer(index, image) => {
                let texture = image
                    .filter(|l| l.image.width > 0 && l.image.height > 0)
                    .map(|l| LayerTexture {
                        x: l.x as f32,
                        y: l.y as f32,
                        texture: preview::texture(l.image),
                    });
                self.state.borrow_mut().textures.insert(index, texture);
                self.refresh_preview();
            }
        }
    }

    // ── Opening ────────────────────────────────────────────────────────────

    fn choose_psd(self: &Rc<Self>) {
        let filter = gtk::FileFilter::new();
        filter.set_name(Some("Photoshop (*.psd)"));
        filter.add_suffix("psd");
        let filters = gio::ListStore::new::<gtk::FileFilter>();
        filters.append(&filter);
        let dialog = gtk::FileDialog::builder()
            .title("Ouvrir un PSD")
            .filters(&filters)
            .default_filter(&filter)
            .build();
        let this = Rc::downgrade(self);
        dialog.open(Some(&self.window), gio::Cancellable::NONE, move |file| {
            let Some(this) = this.upgrade() else { return };
            match file {
                Ok(file) => match file.path() {
                    Some(path) => this.open(path),
                    None => this.error("Fichier non local", &file.uri()),
                },
                Err(e) if e.matches(gtk::DialogError::Dismissed) => {}
                Err(e) => this.error("Sélecteur de fichiers indisponible", &e.to_string()),
            }
        });
    }

    pub fn open(self: &Rc<Self>, psd: PathBuf) {
        self.start(move |this| {
            this.log(&format!("ouverture de {}", psd.display()));
            let path = psd.clone();
            this.run_job(
                false,
                move |session, tx, _| {
                    let _ = tx.send(Msg::Step(format!("lecture de {}", path.display())));
                    session.open(&path).map_err(|e| e.to_string())
                },
                move |this, result| match result {
                    Ok(tree) => this.show_psd(psd, tree),
                    Err(e) => this.error("Impossible d'ouvrir ce fichier", &e),
                },
            );
        });
    }

    fn show_psd(self: &Rc<Self>, psd: PathBuf, tree: LayerTree) {
        let (recipe, saves, problem) = match Recipe::load(&psd, &tree) {
            Ok(recipe) => (recipe, true, None),
            Err(e) => {
                let problem = format!("Recette illisible, modifications non enregistrées : {e}");
                (Recipe::default_for(&psd, &tree), false, Some(problem))
            }
        };
        if let Some(note) = tree.conversion_note() {
            self.log(&note);
            self.toast(&note);
        }
        for layer in tree
            .layers
            .iter()
            .filter(|l| !preview::blend_supported(&l.blend))
        {
            self.log(&format!(
                "aperçu : mode « {} » du calque « {} » affiché en normal (l'export est exact)",
                layer.blend, layer.name
            ));
        }
        self.log(&format!(
            "{} : {}×{}, {} calques",
            psd.display(),
            tree.width,
            tree.height,
            tree.layers.len()
        ));
        self.title
            .set_subtitle(&psd.file_name().unwrap_or_default().to_string_lossy());
        self.build_layers(&tree);
        {
            let mut s = self.state.borrow_mut();
            s.scale = (PREVIEW_SIDE / f64::from(tree.width.max(tree.height))).min(1.0);
            s.checked = vec![false; tree.layers.len()];
            s.textures.clear();
            s.composite = None;
            s.compressed = None;
            s.preview_stopped = false;
            s.recipe_saves = saves;
            s.recipe_problem = problem;
            s.recipe = Some(recipe);
            s.tree = Some(tree);
            s.psd = Some(psd);
            s.current = 0;
        }
        self.fill_outputs();
        self.select_output(0);
        self.pages.set_visible_child_name("main");
        self.set_has_psd(true);
    }

    /// Photoshop's order: top layer first, a group above its content.
    fn build_layers(self: &Rc<Self>, tree: &LayerTree) {
        self.layers.remove_all();
        let checks: Vec<_> = tree
            .layers
            .iter()
            .map(|_| gtk::CheckButton::new())
            .collect();
        for index in display_order(tree, None) {
            let layer = &tree.layers[index];
            let check = &checks[index];
            let row = gtk::Box::builder()
                .spacing(8)
                .margin_top(6)
                .margin_bottom(6)
                .margin_end(12)
                .margin_start(12 + 20 * layer.depth() as i32)
                .build();
            row.append(check);
            if layer.is_group() {
                row.append(&gtk::Image::from_icon_name("folder-symbolic"));
            }
            let name = gtk::Label::builder()
                .label(&layer.name)
                .xalign(0.0)
                .hexpand(true)
                .ellipsize(pango::EllipsizeMode::End)
                .tooltip_text(layer.path_string())
                .build();
            row.append(&name);
            let mut notes = Vec::new();
            if !matches!(layer.kind.as_str(), "pixel" | "group") {
                notes.push(layer.kind.as_str());
            }
            if layer.has_effects {
                notes.push("effets");
            }
            if !layer.visible {
                notes.push("masqué dans le PSD");
            }
            if !notes.is_empty() {
                let note = gtk::Label::new(Some(&notes.join(" · ")));
                note.add_css_class("dim-label");
                note.add_css_class("caption");
                row.append(&note);
            }
            let this = Rc::downgrade(self);
            check.connect_toggled(move |check| {
                if let Some(this) = this.upgrade()
                    && !this.updating.get()
                {
                    this.toggle_layer(index, check.is_active());
                }
            });
            self.layers.append(&row);
        }
        *self.checks.borrow_mut() = checks;
    }

    // ── Outputs ────────────────────────────────────────────────────────────

    fn fill_outputs(&self) {
        self.updating.set(true);
        self.outputs.remove_all();
        let s = self.state.borrow();
        for output in s.recipe.iter().flat_map(|r| &r.outputs) {
            let label = gtk::Label::builder()
                .label(&output.file)
                .xalign(0.0)
                .margin_top(10)
                .margin_bottom(10)
                .margin_start(12)
                .margin_end(12)
                .ellipsize(pango::EllipsizeMode::Middle)
                .build();
            self.outputs.append(&label);
        }
        self.outputs
            .select_row(self.outputs.row_at_index(s.current as i32).as_ref());
        self.updating.set(false);
    }

    /// Shows output `i`: its layers, settings and warnings.
    fn select_output(self: &Rc<Self>, i: usize) {
        let warnings = {
            let mut s = self.state.borrow_mut();
            let State {
                tree,
                recipe,
                checked,
                current,
                ..
            } = &mut *s;
            let (Some(tree), Some(recipe)) = (tree.as_ref(), recipe.as_ref()) else {
                return;
            };
            let Some(output) = recipe.outputs.get(i) else {
                return;
            };
            *current = i;
            let resolution = output.resolve(tree);
            checked.fill(false);
            for &index in &resolution.visible {
                checked[index] = true;
            }
            self.updating.set(true);
            for (check, &on) in self.checks.borrow().iter().zip(checked.iter()) {
                check.set_active(on);
            }
            let format = FORMATS.iter().position(|f| *f == output.format);
            self.format.set_selected(format.unwrap_or(0) as u32);
            // shortcut: a size outside the list (hand-edited recipe) shows as "Source" until changed.
            let size = SIZES.iter().position(|s| *s == output.size);
            self.size.set_selected(size.unwrap_or(0) as u32);
            self.mipmaps.set_active(output.mipmaps);
            self.updating.set(false);
            resolution.warnings()
        };
        for warning in &warnings {
            self.log(&format!("avertissement : {warning}"));
        }
        self.update_banner(&warnings);
        self.update_out_dir();
        self.refresh_preview();
    }

    fn update_banner(&self, warnings: &[String]) {
        let s = self.state.borrow();
        let title = match (&s.recipe_problem, warnings) {
            (Some(problem), _) => problem.clone(),
            (None, []) => String::new(),
            (None, [one]) => one.clone(),
            (None, many) => format!("{} (+{} autres, voir le journal)", many[0], many.len() - 1),
        };
        self.banner.set_title(&title);
        self.banner.set_revealed(!title.is_empty());
    }

    fn toggle_layer(self: &Rc<Self>, index: usize, on: bool) {
        let mut ancestors = Vec::new();
        {
            let mut s = self.state.borrow_mut();
            let State {
                tree,
                recipe,
                checked,
                current,
                ..
            } = &mut *s;
            let (Some(tree), Some(recipe)) = (tree.as_ref(), recipe.as_mut()) else {
                return;
            };
            checked[index] = on;
            // A layer inside an unchecked group would not be drawn: check the groups too.
            let mut parent = if on { tree.layers[index].parent } else { None };
            while let Some(group) = parent {
                if !checked[group] {
                    checked[group] = true;
                    ancestors.push(group);
                }
                parent = tree.layers[group].parent;
            }
            let visible: Vec<usize> = (0..checked.len()).filter(|&i| checked[i]).collect();
            recipe.outputs[*current].set_visible(tree, &visible);
        }
        self.updating.set(true);
        for group in ancestors {
            self.checks.borrow()[group].set_active(true);
        }
        self.updating.set(false);
        self.save();
        self.refresh_preview();
    }

    fn edit_output(self: &Rc<Self>, edit: impl FnOnce(&mut recipe::Output)) {
        if self.updating.get() {
            return;
        }
        {
            let mut s = self.state.borrow_mut();
            let current = s.current;
            let Some(output) = s.recipe.as_mut().and_then(|r| r.outputs.get_mut(current)) else {
                return;
            };
            edit(output);
        }
        self.save();
        self.refresh_compressed();
    }

    fn add_output(self: &Rc<Self>, duplicate: bool) {
        {
            let mut s = self.state.borrow_mut();
            let State {
                psd,
                tree,
                recipe,
                current,
                ..
            } = &mut *s;
            let (Some(psd), Some(tree), Some(recipe)) =
                (psd.as_ref(), tree.as_ref(), recipe.as_mut())
            else {
                return;
            };
            let mut output = if duplicate {
                recipe.outputs[*current].clone()
            } else {
                Recipe::default_for(psd, tree).outputs.remove(0)
            };
            let stem = output.file[..output.file.len() - ".dds".len()].to_string();
            let stem = if duplicate {
                format!("{stem}-copie")
            } else {
                stem
            };
            output.file = unique_name(recipe, &psd_name(psd), &stem);
            recipe.outputs.push(output);
            *current = recipe.outputs.len() - 1;
        }
        let current = self.state.borrow().current;
        self.fill_outputs();
        self.select_output(current);
        self.save();
    }

    fn rename_output(self: &Rc<Self>) {
        let Some(name) = self.current_output_name() else {
            return;
        };
        let entry = gtk::Entry::builder()
            .text(&name)
            .activates_default(true)
            .build();
        let dialog = adw::AlertDialog::new(Some("Renommer la sortie"), Some("Nom du fichier DDS"));
        dialog.set_extra_child(Some(&entry));
        dialog.add_response("cancel", "Annuler");
        dialog.add_response("rename", "Renommer");
        dialog.set_response_appearance("rename", adw::ResponseAppearance::Suggested);
        dialog.set_default_response(Some("rename"));
        let this = Rc::downgrade(self);
        dialog.connect_response(Some("rename"), move |_, _| {
            let Some(this) = this.upgrade() else { return };
            let mut name = entry.text().trim().to_string();
            if !name.to_lowercase().ends_with(".dds") {
                name.push_str(".dds");
            }
            let refused = {
                let mut s = this.state.borrow_mut();
                let State {
                    psd,
                    recipe,
                    current,
                    ..
                } = &mut *s;
                let (Some(psd), Some(recipe)) = (psd.as_ref(), recipe.as_mut()) else {
                    return;
                };
                let others = recipe
                    .outputs
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| i != current)
                    .map(|(_, o)| o.file.as_str());
                match recipe::validate_output_name(&name, &psd_name(psd), others) {
                    Ok(()) => {
                        recipe.outputs[*current].file = name;
                        None
                    }
                    Err(e) => Some(e.to_string()),
                }
            };
            match refused {
                Some(e) => this.error("Nom refusé", &e),
                None => {
                    this.fill_outputs();
                    this.save();
                }
            }
        });
        dialog.present(Some(&self.window));
    }

    fn delete_output(self: &Rc<Self>) {
        let Some(name) = self.current_output_name() else {
            return;
        };
        if self
            .state
            .borrow()
            .recipe
            .as_ref()
            .map_or(0, |r| r.outputs.len())
            < 2
        {
            self.toast("La recette garde au moins une sortie");
            return;
        }
        let dialog = adw::AlertDialog::new(
            Some("Supprimer la sortie ?"),
            Some(&format!(
                "« {name} » sera retirée de la recette. Le fichier DDS déjà écrit reste en place."
            )),
        );
        dialog.add_response("cancel", "Annuler");
        dialog.add_response("delete", "Supprimer");
        dialog.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
        let this = Rc::downgrade(self);
        dialog.connect_response(Some("delete"), move |_, _| {
            let Some(this) = this.upgrade() else { return };
            {
                let mut s = this.state.borrow_mut();
                let current = s.current;
                if let Some(recipe) = s.recipe.as_mut() {
                    recipe.outputs.remove(current);
                }
                s.current = current.saturating_sub(1);
            }
            let current = this.state.borrow().current;
            this.fill_outputs();
            this.select_output(current);
            this.save();
        });
        dialog.present(Some(&self.window));
    }

    fn current_output_name(&self) -> Option<String> {
        let s = self.state.borrow();
        Some(s.recipe.as_ref()?.outputs.get(s.current)?.file.clone())
    }

    fn choose_out_dir(self: &Rc<Self>) {
        let dir = {
            let s = self.state.borrow();
            match (&s.psd, &s.recipe) {
                (Some(psd), Some(recipe)) => recipe.output_dir(psd),
                _ => return,
            }
        };
        let dialog = gtk::FileDialog::builder()
            .title("Dossier de sortie")
            .initial_folder(&gio::File::for_path(&dir))
            .build();
        let this = Rc::downgrade(self);
        dialog.select_folder(Some(&self.window), gio::Cancellable::NONE, move |folder| {
            if let (Some(path), Some(this)) = (folder.ok().and_then(|f| f.path()), this.upgrade()) {
                this.set_out_dir(Some(path));
            }
        });
    }

    /// `None`: the PSD's folder (where the skin usually is).
    fn set_out_dir(self: &Rc<Self>, dir: Option<PathBuf>) {
        {
            let mut s = self.state.borrow_mut();
            let psd_dir = s
                .psd
                .as_deref()
                .and_then(Path::parent)
                .map(Path::to_path_buf);
            let dir = dir.filter(|d| Some(d) != psd_dir.as_ref());
            let Some(recipe) = s.recipe.as_mut() else {
                return;
            };
            recipe.output_dir = dir;
        }
        self.update_out_dir();
        self.save();
    }

    fn update_out_dir(&self) {
        let s = self.state.borrow();
        let (Some(psd), Some(recipe)) = (&s.psd, &s.recipe) else {
            return;
        };
        let text = match &recipe.output_dir {
            None => "Dossier du PSD".to_string(),
            Some(_) => recipe.output_dir(psd).display().to_string(),
        };
        self.out_dir.set_subtitle(&glib::markup_escape_text(&text));
        self.out_dir_reset.set_visible(recipe.output_dir.is_some());
    }

    fn save(&self) {
        let result = {
            let s = self.state.borrow();
            match (&s.psd, &s.recipe) {
                (Some(psd), Some(recipe)) if s.recipe_saves => recipe.save(psd),
                _ => return,
            }
        };
        if let Err(e) = result {
            self.toast(&format!("Recette non enregistrée : {e}"));
            self.log(&format!("erreur : {e}"));
        }
    }

    // ── Preview ────────────────────────────────────────────────────────────

    fn set_no_preview(self: &Rc<Self>, on: bool) {
        {
            let mut s = self.state.borrow_mut();
            s.no_preview = on;
            if on {
                s.cancel.store(true, Ordering::Relaxed);
            } else {
                s.preview_stopped = false;
            }
        }
        self.preview_pages
            .set_visible_child_name(if on { "off" } else { "picture" });
        self.refresh_preview();
        if !on && !self.state.borrow().busy {
            self.maybe_extract();
        }
    }

    /// Extracts the layers the preview still lacks, once per PSD (ADR-005).
    fn maybe_extract(self: &Rc<Self>) {
        let (psd, todo, scale) = {
            let s = self.state.borrow();
            if s.busy || s.no_preview || s.preview_stopped {
                return;
            }
            let (Some(psd), Some(tree)) = (&s.psd, &s.tree) else {
                return;
            };
            let todo: Vec<(usize, String)> = tree
                .layers
                .iter()
                .filter(|l| !l.is_group() && !s.textures.contains_key(&l.index))
                .map(|l| (l.index, l.name.clone()))
                .collect();
            if todo.is_empty() {
                return;
            }
            (psd.clone(), todo, s.scale)
        };
        self.run_job(
            true,
            move |session, tx, cancel| {
                session.ensure(&psd).map_err(|e| e.to_string())?;
                let n = todo.len();
                for (k, (index, name)) in todo.into_iter().enumerate() {
                    if cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    let _ = tx.send(Msg::Step(format!(
                        "aperçu : calque « {name} » ({}/{n})",
                        k + 1
                    )));
                    let image = match session.helper.layer(index, scale) {
                        Ok(image) => Some(image),
                        Err(HelperError::Psd(e)) => {
                            let _ = tx.send(Msg::Step(format!("aperçu : « {name} » ignoré : {e}")));
                            None
                        }
                        Err(e) => return Err(e.to_string()),
                    };
                    let _ = tx.send(Msg::Layer(index, image));
                    let _ = tx.send(Msg::Fraction((k + 1) as f64 / n as f64));
                }
                Ok(())
            },
            |this, result| {
                if let Err(e) = result {
                    this.state.borrow_mut().preview_stopped = true;
                    this.log(&format!("aperçu interrompu : {e}"));
                    this.toast("Aperçu interrompu (voir le journal) ; l'export reste possible");
                }
            },
        );
    }

    /// Fuses the checked layers on the GPU.
    fn refresh_preview(self: &Rc<Self>) {
        let composite = {
            let s = self.state.borrow();
            match &s.tree {
                Some(tree) if !s.no_preview => {
                    let node = preview::compose(tree, &s.checked, &s.textures, None, None);
                    let w = (f64::from(tree.width) * s.scale).round() as f32;
                    let h = (f64::from(tree.height) * s.scale).round() as f32;
                    node.and_then(|node| self.render(&node, w, h))
                }
                _ => None,
            }
        };
        self.state.borrow_mut().composite = composite;
        self.refresh_compressed();
    }

    /// "Aperçu compressé": encodes the preview as the output would be, then decodes it.
    fn refresh_compressed(self: &Rc<Self>) {
        let (composite, format, generation) = {
            let mut s = self.state.borrow_mut();
            s.compressed = None;
            s.compress_gen += 1;
            let format = s
                .recipe
                .as_ref()
                .and_then(|r| r.outputs.get(s.current))
                .map(|o| o.format);
            (s.composite.clone(), format, s.compress_gen)
        };
        self.show();
        let (true, Some(composite), Some(format)) =
            (self.compressed.is_active(), composite, format)
        else {
            return;
        };
        let image = preview::download(&composite);
        let this = Rc::downgrade(self);
        worker::background(
            move |_| {
                let format = format.resolve(&image.data);
                let decoded = dds::encode(&image, format, false).and_then(|b| dds::decode(&b));
                decoded.map(|d| (d, format)).map_err(|e| e.to_string())
            },
            |_| {},
            move |result| {
                let Some(this) = this.upgrade() else { return };
                if this.state.borrow().compress_gen != generation {
                    return; // the layers or the format changed meanwhile
                }
                match result {
                    Ok((image, format)) => {
                        this.state.borrow_mut().compressed = Some(preview::texture(image));
                        this.log(&format!(
                            "aperçu compressé en {}",
                            format.to_string().to_uppercase()
                        ));
                        this.show();
                    }
                    Err(e) => this.toast(&format!("Aperçu compressé impossible : {e}")),
                }
            },
        );
    }

    fn show(&self) {
        let image = {
            let s = self.state.borrow();
            if self.compressed.is_active() && s.compressed.is_some() {
                s.compressed.clone()
            } else {
                s.composite.clone()
            }
        };
        let shown = image.and_then(|image| {
            let node = preview::view_node(&image, View::from_index(self.view.selected()));
            self.render(&node, image.width() as f32, image.height() as f32)
        });
        self.picture.set_paintable(shown.as_ref());
    }

    fn render(&self, node: &gsk::RenderNode, width: f32, height: f32) -> Option<gdk::Texture> {
        let renderer = self.window.renderer()?;
        let viewport = graphene::Rect::new(0.0, 0.0, width, height);
        Some(renderer.render_texture(node, Some(&viewport)))
    }

    // ── Export ─────────────────────────────────────────────────────────────

    fn export(self: &Rc<Self>, all: bool) {
        let (jobs, dir, psd) = {
            let s = self.state.borrow();
            let (Some(psd), Some(tree), Some(recipe)) = (&s.psd, &s.tree, &s.recipe) else {
                return;
            };
            let dir = recipe.output_dir(psd);
            let outputs: Vec<_> = if all {
                recipe.outputs.iter().collect()
            } else {
                recipe.outputs.get(s.current).into_iter().collect()
            };
            let jobs: Vec<ExportJob> = outputs
                .into_iter()
                .map(|output| {
                    let resolution = output.resolve(tree);
                    ExportJob {
                        file: output.file.clone(),
                        as_saved: tree.is_psd_visibility(&resolution.visible),
                        warnings: resolution.warnings(),
                        visible: resolution.visible,
                        settings: output.settings(),
                        dest: dir.join(&output.file),
                    }
                })
                .collect();
            (jobs, dir, psd.clone())
        };
        if !dir.is_dir() {
            self.error(
                "Dossier de sortie introuvable",
                &format!("{}\nChoisissez un autre dossier de sortie.", dir.display()),
            );
            return;
        }
        let existing: Vec<&str> = jobs
            .iter()
            .filter(|j| j.dest.exists())
            .map(|j| j.file.as_str())
            .collect();
        if existing.is_empty() {
            self.start(move |this| this.run_export(psd, jobs));
            return;
        }
        let dialog = adw::AlertDialog::new(
            Some("Remplacer ?"),
            Some(&format!(
                "Déjà dans {} :\n{}",
                dir.display(),
                existing.join("\n")
            )),
        );
        dialog.add_response("cancel", "Annuler");
        dialog.add_response("replace", "Remplacer");
        dialog.set_response_appearance("replace", adw::ResponseAppearance::Destructive);
        let this = Rc::downgrade(self);
        let jobs = RefCell::new(Some(jobs));
        dialog.connect_response(Some("replace"), move |_, _| {
            if let (Some(this), Some(jobs)) = (this.upgrade(), jobs.take()) {
                let psd = psd.clone();
                this.start(move |this| this.run_export(psd, jobs));
            }
        });
        dialog.present(Some(&self.window));
    }

    fn run_export(self: &Rc<Self>, psd: PathBuf, jobs: Vec<ExportJob>) {
        self.run_job(
            false,
            move |session, tx, _| {
                session.ensure(&psd).map_err(|e| e.to_string())?;
                let mut done = Vec::new();
                for job in jobs {
                    let file = job.file.clone();
                    let report = export::export(
                        &mut session.helper,
                        &job.visible,
                        job.as_saved,
                        job.settings,
                        &job.dest,
                        &mut |step| {
                            let _ = tx.send(Msg::Step(format!("{file} : {step}")));
                        },
                    );
                    match report {
                        Ok(report) => done.push((report, job.warnings)),
                        // Keep what was written: the outputs before this one are fine.
                        Err(e) => return Ok((done, Some(format!("{file} : {e}")))),
                    }
                }
                Ok((done, None))
            },
            |this, result| {
                let (done, failure) = result.unwrap_or_else(|e| (Vec::new(), Some(e)));
                for (report, warnings) in done {
                    this.exported(report, &warnings);
                }
                if let Some(e) = failure {
                    this.error("Échec de l'export", &e);
                }
            },
        );
    }

    fn exported(&self, report: Report, warnings: &[String]) {
        for warning in warnings.iter().chain(&report.warnings) {
            self.log(&format!("avertissement : {warning}"));
        }
        self.log(&format!("écrit : {}", report.path.display()));
        let toast = adw::Toast::builder()
            .title(glib::markup_escape_text(&report.to_string()))
            .button_label("Afficher dans le dossier")
            .timeout(8)
            .build();
        let file = gio::File::for_path(&report.path);
        let window = self.window.clone();
        toast.connect_button_clicked(move |_| {
            gtk::FileLauncher::new(Some(&file)).open_containing_folder(
                Some(&window),
                gio::Cancellable::NONE,
                |_| {},
            );
        });
        self.toasts.add_toast(toast);
    }

    // ── Messages ───────────────────────────────────────────────────────────

    fn log(&self, line: &str) {
        let now = glib::DateTime::now_local().ok();
        let stamp = |f: &str| {
            now.as_ref()
                .and_then(|t| t.format(f).ok())
                .unwrap_or_default()
        };
        let time = stamp("%H:%M:%S");
        eprintln!("[{time}] {line}");
        diagnostics::log(&format!("[{}] {line}", stamp("%Y-%m-%d %H:%M:%S")));
        let buffer = self.log.buffer();
        buffer.insert(&mut buffer.end_iter(), &format!("[{time}] {line}\n"));
        let end = buffer.create_mark(None, &buffer.end_iter(), false);
        self.log.scroll_mark_onscreen(&end);
        buffer.delete_mark(&end);
    }

    fn toast(&self, text: &str) {
        self.toasts
            .add_toast(adw::Toast::new(&glib::markup_escape_text(text)));
    }

    fn error(&self, heading: &str, body: &str) {
        self.log(&format!("erreur : {heading} : {body}"));
        let dialog = adw::AlertDialog::new(Some(heading), Some(body));
        dialog.add_response("report", "Copier le rapport");
        dialog.add_response("ok", "OK");
        dialog.set_default_response(Some("ok"));
        let window = self.window.clone();
        dialog.connect_response(Some("report"), move |_, _| {
            window.clipboard().set_text(&diagnostics::report());
        });
        dialog.present(Some(&self.window));
    }

    /// Aide › Diagnostic: is psd-tools ready, versions, end of the log.
    fn diagnostic(&self) {
        let report = diagnostics::report();
        let text = gtk::TextView::builder()
            .editable(false)
            .monospace(true)
            .wrap_mode(gtk::WrapMode::WordChar)
            .build();
        text.buffer().set_text(&report);
        let scroller = gtk::ScrolledWindow::builder()
            .child(&text)
            .min_content_height(320)
            .min_content_width(560)
            .build();
        let dialog = adw::AlertDialog::new(Some("Diagnostic"), None);
        dialog.set_extra_child(Some(&scroller));
        dialog.add_response("copy", "Copier le rapport");
        dialog.add_response("close", "Fermer");
        dialog.set_default_response(Some("close"));
        let window = self.window.clone();
        dialog.connect_response(Some("copy"), move |_, _| {
            window.clipboard().set_text(&report);
        });
        dialog.present(Some(&self.window));
    }

    fn about(&self) {
        adw::AboutDialog::builder()
            .application_name("Ddsnap")
            .application_icon(APP_ID)
            .version(env!("CARGO_PKG_VERSION"))
            .comments("Choisir des calques PSD et les exporter en textures DDS")
            .license_type(gtk::License::MitX11)
            .build()
            .present(Some(&self.window));
    }
}

fn icon_button(icon: &str, tooltip: &str) -> gtk::Button {
    gtk::Button::builder()
        .icon_name(icon)
        .tooltip_text(tooltip)
        .build()
}

/// Children of `parent`, top first, each group followed by its own content.
fn display_order(tree: &LayerTree, parent: Option<usize>) -> Vec<usize> {
    let children: Vec<_> = tree.layers.iter().filter(|l| l.parent == parent).collect();
    children
        .into_iter()
        .rev()
        .flat_map(|l| {
            let mut order = vec![l.index];
            if l.is_group() {
                order.extend(display_order(tree, Some(l.index)));
            }
            order
        })
        .collect()
}

fn psd_name(psd: &Path) -> String {
    psd.file_name().unwrap_or_default().to_string_lossy().into()
}

/// `stem.dds`, else `stem-2.dds`, `stem-3.dds`… : the first name the recipe accepts.
fn unique_name(recipe: &Recipe, psd_name: &str, stem: &str) -> String {
    (1..1000)
        .map(|n| match n {
            1 => format!("{stem}.dds"),
            n => format!("{stem}-{n}.dds"),
        })
        .find(|name| {
            let others = recipe.outputs.iter().map(|o| o.file.as_str());
            recipe::validate_output_name(name, psd_name, others).is_ok()
        })
        .unwrap_or_else(|| "sortie.dds".into())
}
