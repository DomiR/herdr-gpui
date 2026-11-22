//! Optional behaviors under `[features]`. Each flag is declared once below,
//! which generates its config field, its `Feature` variant, and the messages
//! (`features.ftl`) Settings > General lists it with. An entry may carry
//! attributes, such as a `#[cfg]` naming the Cargo feature it needs: a
//! build without it has no field, variant, or row, and ignores the key.
use serde::Deserialize;

macro_rules! features {
    ($($(#[$attr:meta])* $field:ident: $variant:ident = $default:expr, $message:literal, $description:literal;)+) => {
        /// Optional behaviors the config file turns on or off. Each flag
        /// declares its own default, so a missing or empty `[features]` table
        /// is the shipped experience, and a table naming one flag leaves the
        /// others at theirs.
        #[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
        #[serde(default)]
        pub struct Features {
            $($(#[$attr])* #[doc = concat!("Described by the `", $description, "` message.")] pub $field: bool,)+
        }

        impl Default for Features {
            fn default() -> Self {
                Self {
                    $($(#[$attr])* $field: $default,)+
                }
            }
        }

        /// One `[features]` flag, for code that lists or edits them all.
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum Feature {
            $($(#[$attr])* $variant,)+
        }

        impl Feature {
            /// Every flag, alphabetical by key, as Settings lists them.
            pub const ALL: &[Self] = &[$($(#[$attr])* Self::$variant,)+];

            /// The key under `[features]`.
            pub const fn key(self) -> &'static str {
                match self {
                    $($(#[$attr])* Self::$variant => stringify!($field),)+
                }
            }

            /// The id of the flag's row in Settings > General, which its
            /// search entry and debug selector share.
            pub const fn setting_id(self) -> &'static str {
                match self {
                    $($(#[$attr])* Self::$variant => concat!("general-feature-", stringify!($field)),)+
                }
            }

            /// The message naming the flag.
            pub const fn message(self) -> &'static str {
                match self {
                    $($(#[$attr])* Self::$variant => $message,)+
                }
            }

            /// The message describing the flag.
            pub const fn description_message(self) -> &'static str {
                match self {
                    $($(#[$attr])* Self::$variant => $description,)+
                }
            }
        }

        impl Features {
            pub fn enabled(&self, feature: Feature) -> bool {
                match feature {
                    $($(#[$attr])* Feature::$variant => self.$field,)+
                }
            }
        }
    };
}

// Settings lists the flags in this order, alphabetical by key, so additions
// from parallel branches land on different lines.
features! {
    // keep-sorted start
    acp_chats: AcpChats = true, "features-acp-chats",
        "features-acp-chats-description";
    // `sheprd mcp`, the MCP server agents and other clients start
    // (`docs/mcp-plan.md`). The app refuses its requests while this is off.
    #[cfg(feature = "mcp")]
    mcp: Mcp = false, "features-mcp",
        "features-mcp-description";
    // Off on Windows until Moonframe finds its folders there without `HOME`;
    // see `luau_window::NAME`.
    moonframe_window: MoonframeWindow = cfg!(not(windows)), "features-moonframe-window",
        "features-moonframe-window-description";
    qa_menu: QaMenu = false, "features-qa-menu",
        "features-qa-menu-description";
    sidebar_hover_menu: SidebarHoverMenu = false, "features-sidebar-hover-menu",
        "features-sidebar-hover-menu-description";
    // keep-sorted end
}
