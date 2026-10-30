//! `mod list`, `mod reload`, and `mod logs`.
use crate::control::protocol::{
    Call, Response,
    mods::{MAX_LOG_LINES, ModInfo, ModOp, Reply},
};
use gpui::App;

pub(super) fn serve(call: Call<ModOp>, cx: &mut App) -> Response {
    use super::{in_window, not_found};

    let Call { caller, op } = call;
    match op {
        ModOp::List => in_window(&caller, cx, |view, _, _| Response::Mod {
            reply: Reply::Mods {
                dir: view.mods.dir().map(|dir| dir.display().to_string()),
                mods: view
                    .mods
                    .entries()
                    .map(|(name, entry)| ModInfo {
                        name: name.to_owned(),
                        places: entry
                            .places()
                            .iter()
                            .map(|place| place.name().to_owned())
                            .collect(),
                        commands: entry
                            .manifest
                            .iter()
                            .flat_map(|manifest| &manifest.commands)
                            .map(|command| command.id.clone())
                            .collect(),
                        drawn: entry.tree.is_some(),
                        error: entry.error.as_ref().map(ToString::to_string),
                    })
                    .collect(),
            },
        }),
        ModOp::Reload { name } => {
            in_window(&caller, cx, |view, _, _| match view.mods.reload(&name) {
                Ok(()) => Response::Mod {
                    reply: Reply::Reloaded { name },
                },
                Err(error) => not_found(error.to_string()),
            })
        }
        ModOp::Logs { name, lines } => in_window(&caller, cx, |view, _, _| {
            let Some(entry) = view.mods.get(&name) else {
                return not_found(format!("No mod named {name} is running"));
            };
            let lines = lines.min(MAX_LOG_LINES);
            let skip = entry.log.len().saturating_sub(lines);
            Response::Mod {
                reply: Reply::Logs {
                    name,
                    lines: entry
                        .log
                        .iter()
                        .skip(skip)
                        .map(|line| line.text.clone())
                        .collect(),
                },
            }
        }),
    }
}
