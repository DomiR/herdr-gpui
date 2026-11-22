//! What an agent does in a page beyond reading it: run a script of its
//! own (`browser eval`), read the console every page keeps
//! (`browser logs`), and forget what WebKit stores for the page's site
//! (`browser clear-data`). macOS only.
//!
//! WebKit is called directly, which needs `unsafe`. wry runs scripts only
//! through `evaluateJavaScript`, which neither awaits a promise nor says
//! what a script threw, and it offers no way into the website data store.
//! `callAsyncJavaScript` awaits the promise a function body returns, and
//! WebKit runs it whatever the page's Content Security Policy says.
//! objc2-web-kit binds these methods but marks them unsafe for their raw
//! pointers and generic arguments; each call says why it holds. Everything
//! around the calls, reading answers and matching sites, is safe code.
#![allow(unsafe_code)]

mod error;

pub(crate) use error::Error;

use block2::RcBlock;
use objc2::{MainThreadMarker, rc::Retained, runtime::AnyObject};
use objc2_foundation::{NSArray, NSError, NSString, ns_string};
use objc2_web_kit::{
    WKContentWorld, WKErrorCode, WKWebView, WKWebsiteDataRecord, WKWebsiteDataStore,
};
use serde::Deserialize;
use std::{cell::Cell, ptr::NonNull, rc::Rc};
use wry::WebViewExtMacOS as _;

/// Run before each page's own scripts, so `browser logs` has a console to
/// read.
pub(crate) const CONSOLE: &str = include_str!("console.js");

/// Reads the console `CONSOLE` keeps, leaving it as it is.
const READ_CONSOLE: &str =
    "(globalThis[Symbol.for('sheprd.console')] || { read: () => null }).read(false)"; // i18n: data
/// Reads the console and empties it.
const TAKE_CONSOLE: &str =
    "(globalThis[Symbol.for('sheprd.console')] || { read: () => null }).read(true)"; // i18n: data

/// The start and end of every script the app runs for a caller. The
/// caller's script is an async arrow function between them; whatever it
/// returns or throws comes back as one JSON document. A page still on its
/// blank first document answers `not_ready` and runs nothing; one still
/// parsing its document is waited for.
// i18n: data
const PRELUDE: &str = "if (location.href === 'about:blank') {
  return JSON.stringify({ kind: 'not_ready' });
}
if (document.readyState === 'loading') {
  await new Promise((ready) => document.addEventListener('DOMContentLoaded', ready, { once: true }));
}
try {
  const __sheprdValue = await (";
// i18n: data
const EPILOGUE: &str = ")();
  return JSON.stringify(__sheprdValue === undefined
    ? { kind: 'undefined' }
    : { kind: 'value', value: __sheprdValue });
} catch (__sheprdError) {
  let message;
  try {
    message = String(__sheprdError);
  } catch (_) {
    message = 'an exception that cannot be shown';
  }
  return JSON.stringify({ kind: 'threw', message: message });
}
";

/// What a script handed back.
#[derive(Debug, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum Outcome {
    /// The page was still on its blank first document: nothing ran.
    NotReady,
    Undefined,
    /// A value JSON can hold; `null` for one it cannot, such as a function.
    Value {
        #[serde(default)]
        value: serde_json::Value,
    },
    Threw {
        message: String,
    },
}

/// How a caller's script is read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Form {
    /// One expression, whose value is the result: `document.title`.
    Expression,
    /// Statements of a function body, which `return` the result.
    Body,
}

/// The function body WebKit runs for `script` read as `form`.
fn wrapped(script: &str, form: Form) -> String {
    let inner = match form {
        Form::Expression => {
            let expression = script.trim_end_matches(|c: char| c == ';' || c.is_whitespace());
            format!("async () => (\n{expression}\n)")
        }
        Form::Body => format!("async () => {{\n{script}\n}}"),
    };
    format!("{PRELUDE}{inner}{EPILOGUE}")
}

type Done = Box<dyn FnOnce(Result<Outcome, Error>)>;
type Slot = Rc<Cell<Option<Done>>>;

/// Runs `script` in the page's own world, as an expression when it is one
/// and otherwise as a function body, and hands `done` what it returned or
/// threw. A promise it returns is awaited first. WebKit answers on the main
/// thread; a promise that never settles never answers.
///
/// The form is chosen first, in a world of the app's own, by building each
/// form with the `AsyncFunction` constructor, which throws a catchable
/// `SyntaxError` and runs nothing. Compiling a script that does not parse
/// any other way would fire the page's own error handlers (its
/// `window.onerror`, an error reporter) with "Script error.". When the
/// page's Content Security Policy forbids the constructor there, the
/// expression is tried in the page and the body after it, as WebKit leaves
/// no other way to tell.
pub(crate) fn run(
    view: &wry::WebView,
    script: &str,
    done: impl FnOnce(Result<Outcome, Error>) + 'static,
) {
    let Some(mtm) = MainThreadMarker::new() else {
        return done(Err(Error::NotMainThread));
    };
    let webview: Retained<WKWebView> = Retained::into_super(view.webview());
    let done: Slot = Rc::new(Cell::new(Some(Box::new(done))));
    let forms = Forms {
        expression: wrapped(script, Form::Expression),
        body: wrapped(script, Form::Body),
    };
    let check = match forms.check() {
        Ok(check) => check,
        Err(error) => return finish(&done, Err(error.into())),
    };
    // SAFETY: `worldWithName` requires the main thread, which `mtm` proves.
    let world = unsafe { WKContentWorld::worldWithName(ns_string!("sheprd"), mtm) };
    let page = webview.clone();
    call(&webview, &world, &check, move |result, error| {
        let Some(mtm) = MainThreadMarker::new() else {
            return finish(&done, Err(Error::NotMainThread));
        };
        let checked = match read_as::<Checked>(result, error) {
            Ok(checked) => checked,
            Err(error) => return finish(&done, Err(error)),
        };
        match checked {
            Checked::Expression => in_page(&page, mtm, &forms.expression, done.clone()),
            Checked::Body => in_page(&page, mtm, &forms.body, done.clone()),
            Checked::Syntax { message } => finish(&done, Err(Error::Script { message })),
            Checked::Unchecked => {
                let body = forms.body.clone();
                let again = page.clone();
                let done = done.clone();
                // SAFETY: as in `in_page`.
                let world = unsafe { WKContentWorld::pageWorld(mtm) };
                call(&page, &world, &forms.expression, move |result, error| {
                    if let Some(error) = error
                        && error.code() == WKErrorCode::JavaScriptExceptionOccurred.0
                        && let Some(mtm) = MainThreadMarker::new()
                    {
                        return in_page(&again, mtm, &body, done.clone());
                    }
                    finish(&done, read(result, error));
                });
            }
        }
    });
}

/// The two forms a caller's script may take, wrapped to run.
struct Forms {
    expression: String,
    body: String,
}

impl Forms {
    /// The app's own script that tells which form compiles, without running
    /// either: the sources go in as JSON string literals.
    fn check(&self) -> Result<String, serde_json::Error> {
        let expression = serde_json::to_string(&self.expression)?;
        let body = serde_json::to_string(&self.body)?;
        Ok(format!(
            "{CHECK_PRELUDE}[[\"expression\", {expression}], [\"body\", {body}]]{CHECK_EPILOGUE}"
        ))
    }
}

/// Which form of a caller's script compiles.
#[derive(Debug, PartialEq, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Checked {
    Expression,
    Body,
    /// Neither: the body's `SyntaxError`.
    Syntax {
        message: String,
    },
    /// The page's policy forbids building functions from text.
    Unchecked,
}

// i18n: data
const CHECK_PRELUDE: &str = "const AsyncFunction = (async () => {}).constructor;
let failure = 'SyntaxError';
for (const [kind, source] of ";
// i18n: data
const CHECK_EPILOGUE: &str = ") {
  try {
    new AsyncFunction(source);
    return JSON.stringify({ kind: kind });
  } catch (error) {
    if (!(error instanceof SyntaxError)) {
      return JSON.stringify({ kind: 'unchecked' });
    }
    failure = String(error);
  }
}
return JSON.stringify({ kind: 'syntax', message: failure });
";

/// Runs `source`, a form already known to compile, in the page's own world.
fn in_page(webview: &WKWebView, mtm: MainThreadMarker, source: &str, done: Slot) {
    // SAFETY: `pageWorld` requires the main thread, which `mtm` proves.
    let world = unsafe { WKContentWorld::pageWorld(mtm) };
    call(webview, &world, source, move |result, error| {
        finish(&done, read(result, error));
    });
}

/// Hands `done` the page's console, emptying it when `clear` is set:
/// `null` as the value when the page keeps none.
pub(crate) fn console(
    view: &wry::WebView,
    clear: bool,
    done: impl FnOnce(Result<Outcome, Error>) + 'static,
) {
    run(view, if clear { TAKE_CONSOLE } else { READ_CONSOLE }, done);
}

fn finish(done: &Cell<Option<Done>>, outcome: Result<Outcome, Error>) {
    if let Some(done) = done.take() {
        done(outcome);
    }
}

/// Asks WebKit to run `source` as an async function body in the main frame
/// in `world`, and `answer` what it reports.
fn call(
    webview: &WKWebView,
    world: &WKContentWorld,
    source: &str,
    answer: impl Fn(Option<&AnyObject>, Option<&NSError>) + 'static,
) {
    let handler = RcBlock::new(move |result: *mut AnyObject, error: *mut NSError| {
        // SAFETY: WebKit passes null or objects it keeps alive for the
        // duration of this call; the references do not outlive it.
        let (result, error) = unsafe { (result.as_ref(), error.as_ref()) };
        answer(result, error);
    });
    // SAFETY: on the main thread, where WebKit calls back and `world` was
    // made. No arguments are passed, so the
    // dictionary's generic types cannot be wrong; no frame means the main
    // frame; WebKit retains the heap block until it has called it, once.
    unsafe {
        webview.callAsyncJavaScript_arguments_inFrame_inContentWorld_completionHandler(
            &NSString::from_str(source),
            None,
            None,
            world,
            Some(&handler),
        );
    }
}

/// What a script's answer says: the wrapper's JSON, or WebKit's error.
fn read(result: Option<&AnyObject>, error: Option<&NSError>) -> Result<Outcome, Error> {
    read_as(result, error)
}

fn read_as<T: serde::de::DeserializeOwned>(
    result: Option<&AnyObject>,
    error: Option<&NSError>,
) -> Result<T, Error> {
    if let Some(error) = error {
        return Err(Error::Script {
            message: exception(error),
        });
    }
    let text = result
        .and_then(|result| result.downcast_ref::<NSString>())
        .ok_or(Error::NoAnswer)?;
    parse(&text.to_string())
}

fn parse<T: serde::de::DeserializeOwned>(text: &str) -> Result<T, Error> {
    Ok(serde_json::from_str(text)?)
}

/// WebKit's message for an exception, `SyntaxError: Unexpected token ')'`,
/// or its description of any other failure.
fn exception(error: &NSError) -> String {
    error
        .userInfo()
        .objectForKey(ns_string!("WKJavaScriptExceptionMessage"))
        .and_then(|message| message.downcast_ref::<NSString>().map(NSString::to_string))
        .unwrap_or_else(|| error.localizedDescription().to_string())
}

/// Whether WebKit's record named `record` (a registrable domain such as
/// `example.com`, or a host such as `localhost` or `127.0.0.1`) holds
/// data of `host`.
fn belongs(record: &str, host: &str) -> bool {
    let (record, host) = (record.to_ascii_lowercase(), host.to_ascii_lowercase());
    !record.is_empty() && (host == record || host.ends_with(&format!(".{record}")))
}

/// Forgets every kind of data WebKit keeps for `host` in the page's data
/// store (cache, cookies, storage, service workers) on every port, then
/// hands `done` the names of the records cleared.
pub(crate) fn clear_site(
    view: &wry::WebView,
    host: &str,
    done: impl FnOnce(Result<Vec<String>, Error>) + 'static,
) {
    let Some(mtm) = MainThreadMarker::new() else {
        return done(Err(Error::NotMainThread));
    };
    // SAFETY: plain getters on the main thread, which `mtm` proves.
    let (store, types) = unsafe {
        (
            view.webview().configuration().websiteDataStore(),
            WKWebsiteDataStore::allWebsiteDataTypes(mtm),
        )
    };
    let host = host.to_owned();
    let (remover, kinds) = (store.clone(), types.clone());
    let done = Cell::new(Some(done));
    let fetched = RcBlock::new(move |records: NonNull<NSArray<WKWebsiteDataRecord>>| {
        // SAFETY: WebKit passes an array it keeps alive for this call.
        let records = unsafe { records.as_ref() };
        let matching: Vec<Retained<WKWebsiteDataRecord>> = records
            .iter()
            // SAFETY: a plain getter on a record WebKit handed over.
            .filter(|record| belongs(&unsafe { record.displayName() }.to_string(), &host))
            .collect();
        // SAFETY: as above.
        let names: Vec<String> = matching
            .iter()
            .map(|record| unsafe { record.displayName() }.to_string())
            .collect();
        let Some(done) = done.take() else {
            return;
        };
        if matching.is_empty() {
            return done(Ok(names));
        }
        let pending = Cell::new(Some((done, names)));
        let removed = RcBlock::new(move || {
            if let Some((done, names)) = pending.take() {
                done(Ok(names));
            }
        });
        // SAFETY: on the main thread, where WebKit calls this handler; the
        // array and types live for the call, and WebKit retains the block.
        unsafe {
            remover.removeDataOfTypes_forDataRecords_completionHandler(
                &kinds,
                &NSArray::from_retained_slice(&matching),
                &removed,
            );
        }
    });
    // SAFETY: on the main thread; WebKit retains the block until it calls it.
    unsafe { store.fetchDataRecordsOfTypes_completionHandler(&types, &fetched) };
}

#[cfg(test)]
mod tests;
