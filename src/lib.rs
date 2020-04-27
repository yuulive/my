extern crate xcb;

use std::collections::HashMap;
use xcb::xproto;
use std::error::Error;
use std::cmp::max;

pub enum Actions {
    SwitchWindow, CloseWindow, ChangeLayout,
}

pub enum Meta {
    Mod1, Mod4
}

enum Layout {
    BSPV,
    Monocle,
    BSPH
}

type Window = u32;

pub type Key = char;

type WorkspaceName = Key;

pub type CustomAction = Box<dyn Fn() -> ()>;

type Color = u32;

#[derive(Clone)]
struct Geometry(u32, u32, u32, u32);

pub struct WindowBorder {
    pub width: u32,
    pub gap: u32,
    pub focus_color: Color,
    pub normal_color: Color,
}

struct Workspace {
    layout: Layout,
    windows: Vec<Window>,
    focus: usize,
}

pub struct DisplayBorder {
    pub left: u32,
    pub right: u32,
    pub bottom: u32,
    pub top: u32,
}

pub struct Conf {
    pub meta: Meta,
    pub border: WindowBorder,
    pub display_border: DisplayBorder,
    pub workspaces_names: Vec<WorkspaceName>,
    pub custom_actions: HashMap<Key, CustomAction>,
    pub wm_actions: HashMap<Key, Actions>,
    pub float_classes: Vec<String>,
    pub auto_float_types: Vec<String>,
}

#[derive(Clone)]
struct MouseMoveStart {
    root_x: i16,
    root_y: i16,
    child: Window,
    detail: u8,
}

pub struct YazgooWM {
    conf: Conf,
    current_workspace: WorkspaceName,
    float_windows: Vec<Window>,
    workspaces: HashMap<WorkspaceName, Workspace>,
    conn: xcb::Connection,
    mouse_move_start: Option<MouseMoveStart>,
    button_press_geometry: Option<Geometry>,
}

fn keycode_to_key(keycode: u8) -> Option<Key> {
    let mut translator = HashMap::new();
    translator.insert(38, 'a');
    translator.insert(39, 'u');
    translator.insert(40, 'i');
    translator.insert(27, 'o');
    translator.insert(26, 'p');
    translator.insert(65, ' ');
    translator.insert(61, 'f');
    translator.insert(46, 'r');
    translator.insert(35, 'w');
    translator.insert(44, 't');
    translator.insert(58, 'q');
    match translator.get(&keycode) {
        Some(x) => Some(*x),
        None => None
    }
}

fn key_to_keycode(key: &Key) -> Option<u8> {
    let mut translator = HashMap::new();
    translator.insert('a', 38);
    translator.insert('u', 39);
    translator.insert('i', 40);
    translator.insert('o', 27);
    translator.insert('p', 26);
    translator.insert(' ', 65);
    translator.insert('f', 61);
    translator.insert('r', 46);
    translator.insert('w', 35);
    translator.insert('t', 44);
    translator.insert('q', 58);
    match translator.get(key) {
        Some(x) => Some(*x),
        None => None
    }
}


fn unmap_workspace_windows(conn: &xcb::Connection, windows: &mut Vec<Window>, focus: usize, move_window: bool) -> Option<Window> {
    let mut window_to_move = None;
    for (i, window) in windows.iter().enumerate() {
        if move_window && i == focus {
            window_to_move = Some(*window);
        }
        else {
            xcb::unmap_window(conn, *window);
        }
    }
    window_to_move
}

fn change_workspace(conn: &xcb::Connection, workspaces: &mut HashMap<WorkspaceName, Workspace>, previous_workspace: WorkspaceName, next_workspace: WorkspaceName, move_window: bool) -> Result<WorkspaceName, Box<dyn Error>> {
    let workspace = workspaces.get_mut(&previous_workspace).ok_or("workspace not found")?;
    let window_to_move = unmap_workspace_windows(conn, &mut workspace.windows, workspace.focus, move_window);
    match window_to_move {
        Some(w) => {
            workspace.windows.retain( |x| *x != w );
            if workspace.windows.len() > 0 {
                workspace.focus = workspace.windows.len() - 1;
            }
            else {
                workspace.focus = 0;
            }
        },
        None => {},
    };
    let workspace = workspaces.get_mut(&next_workspace).ok_or("workspace not found")?;
    for window in &workspace.windows {
        xcb::map_window(conn, *window);
    }
    window_to_move.map( 
        |w| { 
            workspace.windows.push(w);
            workspace.focus = workspace.windows.len() - 1;
        }
    );
    Ok(next_workspace)
}

    fn geometries_bsp(i: usize, window_count: usize, left: u32, top: u32, width: u32, height: u32, vertical: usize) -> Vec<Geometry> {
        if window_count == 0 {
            vec![]
        }
        else if window_count == 1 {
            vec![Geometry(left, top, width, height)]
        }
        else if i % 2 == vertical {
            let mut res = vec![Geometry(left, top, width, height / 2)];
            res.append(
                &mut geometries_bsp(i + 1, window_count - 1, left, top + height / 2, width, height / 2, vertical));
            res
        }
        else {
            let mut res = vec![Geometry(left, top, width / 2, height)];
            res.append(
                &mut geometries_bsp(i + 1, window_count - 1, left + width / 2, top, width / 2, height, vertical));
            res
        }
    }

    fn resize_workspace_windows(conn: &xcb::Connection,workspace: &Workspace, border: &WindowBorder, display_border: &DisplayBorder, float_windows: &Vec<u32>) {
        let mut non_float_windows = workspace.windows.clone();
        non_float_windows.retain(|w| float_windows.contains(&w));
        let count = non_float_windows.len();
        if count == 0 {
            return
        }
        let screen = conn.get_setup().roots().nth(0).unwrap();
        let width = screen.width_in_pixels() as u32 - display_border.right - display_border.left;
        let height = screen.height_in_pixels() as u32 - display_border.top - display_border.bottom;
        let geos = match workspace.layout {
            Layout::BSPV => {
                geometries_bsp(0, count, display_border.left, display_border.top, width, height, 1)},
            Layout::BSPH => {
                geometries_bsp(0, count, display_border.left, display_border.top, width, height, 0)},
            Layout::Monocle => {
                geometries_bsp(0, 1, display_border.left, display_border.top, width, height, 1)},
        };
        match workspace.layout {
            Layout::BSPV | Layout::BSPH => {
                for (i, geo) in geos.iter().enumerate() {
                    match non_float_windows.get(i) {
                        Some(window) => {xcb::configure_window(&conn, *window, &[
                            (xcb::CONFIG_WINDOW_X as u16, geo.0 + border.gap),
                            (xcb::CONFIG_WINDOW_Y as u16, geo.1 + border.gap),
                            (xcb::CONFIG_WINDOW_WIDTH as u16, geo.2 - 2 * border.width - 2 * border.gap),
                            (xcb::CONFIG_WINDOW_HEIGHT as u16, geo.3 - 2 * border.width - 2 * border.gap),
                            (xcb::CONFIG_WINDOW_BORDER_WIDTH as u16, border.width),
                        ]
                        );
                        },
                        None => {}
                    }
                }
            }
            Layout::Monocle => {
                match workspace.windows.get(workspace.focus) {
                    Some(window) => {xcb::configure_window(&conn, *window, &[
                        (xcb::CONFIG_WINDOW_X as u16, geos[0].0 + border.gap),
                        (xcb::CONFIG_WINDOW_Y as u16, geos[0].1 + border.gap),
                        (xcb::CONFIG_WINDOW_WIDTH as u16, geos[0].2 - 2 * border.width - 2 * border.gap),
                        (xcb::CONFIG_WINDOW_HEIGHT as u16, geos[0].3 - 2 * border.width - 2 * border.gap),
                        (xcb::CONFIG_WINDOW_BORDER_WIDTH as u16, border.width),
                        (xcb::CONFIG_WINDOW_STACK_MODE as u16, xcb::STACK_MODE_ABOVE),
                    ]
                    );
                    },
                    None => {}
                }
            }
        }
        for (i, geo) in workspace.windows.iter().enumerate() {
            match workspace.windows.get(i) {
                Some(window) => {
                    xcb::change_window_attributes(&conn, *window, &[
                        (xcb::CW_BORDER_PIXEL, 
                         if i == workspace.focus {
                             border.focus_color
                         } else {
                             border.normal_color
                         }
                        ),
                    ]);
                    if i == workspace.focus {
                        xcb::set_input_focus(&conn, xcb::INPUT_FOCUS_PARENT as u8, *window, 0);
                    }
                },
                None =>{}
            }
        }
    }

    fn window_types_from_list(conn: &xcb::Connection, types_names: &Vec<String>) -> Vec<xcb::Atom> {
        types_names.into_iter().map(|x| {
            let name = format!("_NET_WM_WINDOW_TYPE_{}", x.to_uppercase());
            let res = xcb::intern_atom(&conn, true, name.as_str()).get_reply().map(|x| x.atom());
            res.ok()
        }
        ).flatten().collect()
    }


impl YazgooWM {

    fn init(&mut self) {
        let screen = self.conn.get_setup().roots().nth(0).unwrap();
        let mod_key = match self.conf.meta {
             Meta::Mod4 => xcb::MOD_MASK_4,
             Meta::Mod1 => xcb::MOD_MASK_1
        };
        for mod_mask in vec![mod_key, mod_key | xcb::MOD_MASK_SHIFT] {
            for workspace_name in &self.conf.workspaces_names {
                xcb::grab_key(&self.conn, false, screen.root(), mod_mask as u16, key_to_keycode(workspace_name).unwrap(), xcb::GRAB_MODE_ASYNC as u8, xcb::GRAB_MODE_ASYNC as u8);
            }
            for custom_action_key in self.conf.custom_actions.keys() {
                xcb::grab_key(&self.conn, false, screen.root(), mod_mask as u16, key_to_keycode(custom_action_key).unwrap(), xcb::GRAB_MODE_ASYNC as u8, xcb::GRAB_MODE_ASYNC as u8);
            }
            for custom_action_key in self.conf.wm_actions.keys() {
                xcb::grab_key(&self.conn, false, screen.root(), mod_mask as u16, key_to_keycode(custom_action_key).unwrap(), xcb::GRAB_MODE_ASYNC as u8, xcb::GRAB_MODE_ASYNC as u8);
            }
        }
        for button in vec![1, 3] {
            xcb::grab_button(&self.conn, false, screen.root(), (xcb::EVENT_MASK_BUTTON_PRESS | xcb::EVENT_MASK_BUTTON_RELEASE | xcb::EVENT_MASK_POINTER_MOTION) as u16, xcb::GRAB_MODE_ASYNC as u8, xcb::GRAB_MODE_ASYNC as u8, xcb::NONE, xcb::NONE, button as u8, mod_key as u16);
        }
        xcb::change_window_attributes(&self.conn, screen.root(), &[(xcb::CW_EVENT_MASK, xcb::EVENT_MASK_SUBSTRUCTURE_NOTIFY as u32)]);
        self.conn.flush();
    }

    fn run_wm_action(&mut self, key: &Key) -> Result<(), Box<dyn Error>> {
        let action = self.conf.wm_actions.get(&key).ok_or("action not found")?;
        let workspace = self.workspaces.get_mut(&self.current_workspace).ok_or("workspace not found")?;
        match action {
            Actions::CloseWindow => {
                let window = workspace.windows.get(workspace.focus).ok_or("window not found")?;
                xcb::destroy_window(&self.conn, *window);
            },
            Actions::SwitchWindow => {
                if workspace.windows.len() > 0 {
                    workspace.focus = (workspace.focus + 1) % workspace.windows.len();
                }
            },
            Actions::ChangeLayout => {
                workspace.layout = match workspace.layout {
                    Layout::BSPV => Layout::Monocle,
                    Layout::Monocle => Layout::BSPH,
                    Layout::BSPH => Layout::BSPV,
                }
            },
        };
        resize_workspace_windows(&self.conn, &workspace, &self.conf.border, &self.conf.display_border, &self.float_windows);
        Ok(())
    }



    fn get_str_property(&mut self, window: u32, name: &str) -> Option<String> {
        let _net_wm_window_type = xcb::intern_atom(&self.conn, false, name).get_reply().unwrap().atom();
        let cookie = xcb::get_property(&self.conn, false, window, _net_wm_window_type, xcb::ATOM_ANY, 0, 1024);
        if let Ok(reply) = cookie.get_reply() {
            Some(std::str::from_utf8(reply.value()).unwrap().to_string())
        } else {
            None
        }
    }

    fn get_atom_property(&mut self, id: u32, name: &str) -> Result<u32, Box<dyn Error>> {
        let window: xproto::Window = id;
        let ident = xcb::intern_atom(&self.conn, true, "_NET_WM_WINDOW_TYPE").get_reply()?.atom();
        let reply = xproto::get_property(&self.conn, false, window, ident, xproto::ATOM_ATOM, 0, 1024).get_reply()?;
        if reply.value_len() <= 0 {
            Ok(42)
        }
        else {
            Ok(reply.value()[0])
        }
    }

    fn setup_new_window(&mut self, window: u32) -> Result<(), Box<dyn Error>> {
        let wm_class = self.get_str_property(window, "WM_CLASS").ok_or("failed getting wm class")?;
        let window_type = self.get_atom_property(window, "_NET_WM_WINDOW_TYPE")?;
        let auto_float_types =  window_types_from_list(&self.conn, &self.conf.auto_float_types);
        if auto_float_types.contains(&window_type) {
            return Ok(())
        }
        let wm_class : Vec<&str> = wm_class.split('\0').collect();
        match self.workspaces.get_mut(&self.current_workspace) {
            Some(workspace) => {
                if !workspace.windows.contains(&window) {
                    if wm_class.len() != 0 && ! self.conf.float_classes.contains(&wm_class[0].to_string()) && !self.float_windows.contains(&window) { 
                        self.float_windows.push(window);
                    }
                    workspace.windows.push(window);
                    resize_workspace_windows(&self.conn, &workspace, &self.conf.border, &self.conf.display_border, &self.float_windows);
                }
            },
            None => {
            },
        }
        Ok(())
    }

    fn resize_window(&mut self, event: &xcb::MotionNotifyEvent) -> Result<(), Box<dyn Error>> {
        /* TODO */
        let mouse_move_start = self.mouse_move_start.clone().ok_or("no mouse move start")?;
        let attr = self.button_press_geometry.clone().ok_or("no button press geometry")?;
        let xdiff = event.root_x() - mouse_move_start.root_x;
        let ydiff = event.root_y() - mouse_move_start.root_y;
        let x = attr.0 as i32 + if mouse_move_start.detail == 1 { xdiff as i32 } else { 0 };
        let y = attr.1 as i32 + if mouse_move_start.detail == 1 { ydiff as i32 } else { 0 };
        let width = max(1, attr.2 as i32 + if mouse_move_start.detail == 3 { xdiff as i32 } else { 0 });
        let height = max(1, attr.3 as i32 + if mouse_move_start.detail == 3 { ydiff as i32 } else { 0 });
        xcb::configure_window(&self.conn, mouse_move_start.child, &[
                            (xcb::CONFIG_WINDOW_X as u16, x as u32),
                            (xcb::CONFIG_WINDOW_Y as u16, y as u32),
                            (xcb::CONFIG_WINDOW_WIDTH as u16, width as u32),
                            (xcb::CONFIG_WINDOW_HEIGHT as u16, height as u32),
                        ]);
        Ok(())
    }

    fn destroy_window(&mut self, window: u32) {
        if self.float_windows.contains(&window) {
            self.float_windows.retain(|&x| x != window);
        }
        for (_, workspace) in &mut self.workspaces {
            if workspace.windows.contains(&window) {
                workspace.windows.retain(|&x| x != window);
                resize_workspace_windows(&self.conn, &workspace, &self.conf.border, &self.conf.display_border, &self.float_windows);
                workspace.focus = 0;
            }
        }
    }

    pub fn run(&mut self) {
        loop {
            match self.conn.wait_for_event() {
                Some(event) => {
                    let r = event.response_type();
                    if r == xcb::MAP_NOTIFY as u8 {
                        let map_notify : &xcb::MapNotifyEvent = unsafe {
                            xcb::cast_event(&event)
                        };
                        self.setup_new_window(map_notify.window());
                    }
                    if r == xcb::DESTROY_NOTIFY as u8 {
                        let map_notify : &xcb::DestroyNotifyEvent = unsafe {
                            xcb::cast_event(&event)
                        };
                        self.destroy_window(map_notify.window());
                    }
                    else if r == xcb::BUTTON_PRESS as u8 {
                        let event : &xcb::ButtonPressEvent = unsafe {
                            xcb::cast_event(&event)
                        };
                        match xcb::get_geometry(&self.conn, event.child()).get_reply() {
                            Ok(geometry) => {self.button_press_geometry = Some(
                                Geometry(geometry.x() as u32, geometry.y() as u32, geometry.width() as u32, geometry.height() as u32)
                                );},
                            Err(_) => {},
                        }
                        self.mouse_move_start = Some(MouseMoveStart{
                            root_x: event.root_x(),
                            root_y: event.root_y(),
                            child: event.child(),
                            detail: event.detail(),
                        });
                    }
                    else if r == xcb::MOTION_NOTIFY as u8 {
                        let event : &xcb::MotionNotifyEvent = unsafe {
                            xcb::cast_event(&event)
                        };
                        self.resize_window(event);
                    }
                    else if r == xcb::BUTTON_RELEASE as u8 {
                        self.mouse_move_start = None;
                    }
                    else if r == xcb::KEY_PRESS as u8 {
                        let key_press : &xcb::KeyPressEvent = unsafe {
                            xcb::cast_event(&event)
                        };
                        let keycode = key_press.detail();
                        match &keycode_to_key(keycode) {
                            Some(key) => {
                                if self.conf.workspaces_names.contains(key) {
                                    match change_workspace(&self.conn, &mut self.workspaces, self.current_workspace, *key, (key_press.state() as u32 ) & xcb::MOD_MASK_SHIFT != 0) {
                                        Ok(workspace) => { 
                                            self.current_workspace = workspace;
                                            match self.workspaces.get(&self.current_workspace) {
                                                Some(workspace) => resize_workspace_windows(&self.conn, &workspace, &self.conf.border, &self.conf.display_border, &self.float_windows),
                                                None => {},
                                            }
                                        },
                                        Err(_) => {},
                                    };
                                }
                                else if self.conf.wm_actions.contains_key(&key) {
                                    self.run_wm_action(&key);
                                }
                                else if self.conf.custom_actions.contains_key(&key) {
                                    match self.conf.custom_actions.get(&key) {
                                        Some(action) => {
                                            action()
                                        },
                                        None => {},
                                    }
                                }
                            },
                            None => {
                            },

                        }
                    }
                },
                None => {}
            }
            self.conn.flush();
        }
    }
}

pub fn yazgoowm(conf: Conf) -> YazgooWM {
    let (conn, _) = xcb::Connection::connect(None).unwrap();
    let workspaces = conf.workspaces_names.clone().into_iter().map( |x|
            (x, Workspace {
                layout: Layout::BSPV,
                windows: vec![],
                focus: 0,
        })).into_iter().collect();
    let mut wm = YazgooWM {
        conf: conf,
        current_workspace: 'a',
        float_windows: vec![],
        workspaces: workspaces,
        conn: conn,
        button_press_geometry: None,
        mouse_move_start: None,
    };
    wm.init();
    wm
}