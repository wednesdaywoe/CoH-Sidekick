//! Desktop sign-in: the loopback listener the provider answers to (RB5).
//!
//! **This file exists because it is the one thing that cannot live in [`crate::cloud`].** That
//! directory's rule is that `grep 'target_arch = "wasm32"'` over it prints 0 — no per-target arms,
//! no `None` twins — and a TCP listener is the definition of a thing one target has and the other
//! does not. So the split lives here, at the caller, exactly where
//! [`crate::cloud::oauth`]'s module doc said RB5 would put it: the cloud layer hands out a URL
//! string and takes back what came home, and both halves of that are the same on either target.
//!
//! # PKCE, and the two-request bounce this replaced
//!
//! The desktop flow is PKCE (F62 — [`crate::cloud::oauth`]'s module doc has why, and why the
//! reason it was once rejected for turned out to be false). What that buys *here*, structurally,
//! is that the answer arrives as `?code=` in a **query**, and a query reaches a server.
//!
//! **It did not used to.** Under the implicit flow the provider answers with the tokens in the URL
//! **fragment** — `#access_token=…&refresh_token=…` — and a fragment is client-side only: the
//! browser never sends it to the server. A loopback listener therefore saw `GET /` with nothing on
//! it, and the round trip needed two requests rather than one: a small HTML page whose only job
//! was to copy `window.location.hash` into a second request that *was* a query.
//!
//! That page is gone, and it is worth writing down what went with it, because each piece was a
//! guard rather than a convenience:
//!
//! * **The tokens no longer enter this process from the socket at all.** What arrives is a code,
//!   and a code is inert without the verifier drawn in [`round_trip`] and never published. An
//!   attacker who injects one gets a 400 from GoTrue, not a session — the flow state their code
//!   belongs to carries their challenge, and this side presents its own verifier.
//! * **The nonce is gone with the page it was carried by.** It existed to tell our own bounce
//!   page's request apart from an unsolicited one; with no bounce page there is no second request
//!   to authenticate.
//! * **The page the browser lands on now runs no JavaScript.** It says the sign-in finished. It
//!   reads nothing, echoes nothing, and posts nowhere.
//!
//! # What the listener is careful about
//!
//! * **It binds `127.0.0.1:0`** — an ephemeral port, the loopback interface only, so nothing off
//!   this machine can reach it — and it **dies with the callback**. A listener that outlived the
//!   sign-in would be a port this app holds open for the life of the process for no reason.
//! * **It answers one path and 404s everything else.** The path carries an unguessable segment
//!   (F02), which under the implicit flow was the only `state` binding the flow had. Under PKCE
//!   the verifier is that binding, so the secret path is now the second lock rather than the only
//!   one — kept because it costs one draw and it is what stops a port-guesser from ending a
//!   sign-in that is still in flight.
//! * **It gives up.** A user who closes the browser tab never comes back, and without a deadline
//!   the task would wait for the life of the app.
//! * **It cannot be held.** Every connection is served on its own task, under its own few-second
//!   deadline, with a cap on how many run at once (F64). Before that it read one connection to
//!   its end before accepting the next, and waited on each without a deadline — so a single
//!   socket that opened and said nothing held the accept loop for the whole five minutes and the
//!   real callback was never read. No secret, no port guess, no protocol: a connection and
//!   silence.

use crate::cloud::account::Account;
use crate::cloud::oauth::AuthProvider;

/// How long a sign-in may take before the listener gives up and closes the port.
///
/// Five minutes is a person finding a password manager, not a person who has gone to lunch. The
/// failure it prevents is silent: an abandoned sign-in with no deadline is a task and a bound port
/// held for the life of the process, and nothing would ever say so.
#[cfg(not(target_arch = "wasm32"))]
const DEADLINE: std::time::Duration = std::time::Duration::from_secs(300);

/// How long one connection may hold a slot, in each direction, before it is given up on.
///
/// **This is the half of F64 that is independently correct.** Before it, `read_request` awaited a
/// peer under no obligation to say anything, so a connection that opened and went quiet held the
/// accept loop until `DEADLINE` expired five minutes later. A real callback is a browser on
/// loopback answering a redirect it has already followed — microseconds. Five seconds is not
/// generosity; it is a number far enough above the real case that nothing legitimate reaches it.
///
/// **It caps the write too**, because `respond` is the other end a peer can hold: a socket
/// advertising a zero receive window stalls `write_all` for as long as it likes, and a stalled
/// write holds its slot exactly as a stalled read once held the loop.
#[cfg(not(target_arch = "wasm32"))]
const CONNECTION_DEADLINE: std::time::Duration = std::time::Duration::from_secs(5);

/// How many connections the listener serves at once.
///
/// **The deadline alone does not close F64, it prices it.** Served one at a time, N silent
/// connections still cost N × the deadline, and N is free to an attacker holding ephemeral ports.
/// Serving them alongside each other is the fix, and the cap is what stops the fix from being its
/// own exhaustion — an unbounded spawn per accept hands anything on this machine a file descriptor
/// and a task for the asking.
///
/// At the cap the loop stops calling `accept`, which is back-pressure rather than refusal: waiting
/// connections queue in the kernel's backlog, so a real callback among them is served a deadline
/// late rather than dropped. Sixty-four is an order of magnitude above what a browser opens for
/// one redirect and a rounding error in descriptors.
#[cfg(not(target_arch = "wasm32"))]
const CONCURRENT_CONNECTIONS: usize = 64;

/// Sign in on desktop: bind, send the user to the provider, catch the answer, spend it.
///
/// Reports through [`Account`]'s own refusal channel, like [`Account::sign_in`] does, so a failure
/// here reaches the same modal every other cloud refusal does rather than a second one.
#[cfg(not(target_arch = "wasm32"))]
pub async fn sign_in(account: &Account, provider: AuthProvider) {
    if let Err(reason) = round_trip(account, provider).await {
        account.refuse(format!("Sign in with {}", provider.label()), reason);
    }
}

/// The wasm twin, which is never called: [`crate::main_menu`] routes the web build through
/// [`Account::sign_in`], whose redirect IS the page load. It exists so both arms of that `cfg!`
/// type-check together — the rule the menu's own comment states — and it is a compile-time
/// impossibility rather than a runtime `None`: nothing on this target can reach it.
#[cfg(target_arch = "wasm32")]
pub async fn sign_in(_account: &Account, _provider: AuthProvider) {
    unreachable!("the web build signs in through a page redirect, not a loopback listener");
}

#[cfg(not(target_arch = "wasm32"))]
async fn round_trip(account: &Account, provider: AuthProvider) -> Result<(), String> {
    use tokio::net::TcpListener;

    let listener = TcpListener::bind(("127.0.0.1", 0))
        .await
        .map_err(|e| format!("could not open a local port to sign in through: {e}"))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("could not read the local port: {e}"))?
        .port();

    // **The verifier is the one value here that never leaves this function's stack.** Its hash
    // goes into the URL, the URL goes into a child process's argv, and that argv is readable by
    // anything on this machine — which is the whole of F62. What makes that survivable is that a
    // challenge is public by design and the verifier is not derivable from it.
    let verifier = random_hex(VERIFIER_BYTES)?;
    // Drawn before the authorize URL too, because it goes INTO that URL — see `redirect_address`.
    let secret = random_hex(SECRET_BYTES)?;
    let redirect_to = redirect_address(port, &secret);

    let url = account
        .authorize_url(
            provider,
            &redirect_to,
            &crate::cloud::oauth::pkce_challenge(&verifier),
        )
        .await
        .map_err(|e| e.to_string())?;

    crate::app_actions::open_external(&url);

    let code = tokio::time::timeout(DEADLINE, catch_callback(listener, &secret))
        .await
        .map_err(|_| {
            "the sign-in was not completed in time, so the local port was closed".to_string()
        })??;

    account
        .finish_sign_in(&code, &verifier)
        .await
        .map_err(|e| e.to_string())
}

/// The address the provider is told to send the answer back to. One place, because it is matched
/// remotely against rules nobody here can see.
///
/// **The secret path was the `state` this flow could not otherwise have** (F02), and it is now the
/// second of two locks rather than the only one: a port-guesser who gets past it still cannot
/// produce a code that survives the exchange. It stays because it is what keeps a guessed port
/// from *ending* a sign-in in flight — every path but this one is a 404, so a stray local request
/// cannot hand this listener something it will act on.
///
/// **The allow-list needs no entry for this, and that was measured rather than assumed.** The
/// deployed GoTrue is v2.197.0 (its own `/auth/v1/health` says so), and that version's
/// `IsRedirectURLValid` returns `ip.IsLoopback()` before `URIAllowListMap` is ever consulted
/// (`internal/utilities/request.go`) — so a `127.0.0.1` redirect is accepted at any port and any
/// path. The note that used to stand here said the entry had to become `http://127.0.0.1:*/**`
/// because a glob's `*` stops at a separator. That is true of globs and false of this code path,
/// and it is recorded because it was a pending deployment step that nothing would ever have
/// closed.
#[cfg(not(target_arch = "wasm32"))]
fn redirect_address(port: u16, secret: &str) -> String {
    format!("http://127.0.0.1:{port}/{secret}")
}

/// Wait for the provider's redirect to land on the secret path, and take the code off it.
///
/// Loops rather than accepting a fixed number of times, because a browser opens connections this
/// code did not ask for — a favicon fetch, a speculative preconnect — and each is answered and
/// dropped. Only a `GET` to the secret path carrying a `code` or an `error` ends it.
///
/// **A refusal ends it too, and loudly.** The provider says no by redirecting here with
/// `error_description`, and the alternative to reading it is a user staring at a browser tab for
/// five minutes while a listener waits for a code that is never coming.
///
/// **Accepting is not serving, and F64 is what conflating them cost.** This used to read each
/// connection to its end before accepting the next, so the slowest peer set the pace for everyone
/// queued behind it — and a peer that said nothing at all set it to `DEADLINE`. Now `accept` hands
/// the socket to a task ([`serve`]) and goes straight back to the queue; the loop's other arm
/// collects those tasks, and the first outcome one of them returns ends the sign-in. A connection
/// can no longer hold the door, only a slot, and only for [`CONNECTION_DEADLINE`].
///
/// A handler that panics arrives here as a `JoinError` rather than as an unwind through the
/// sign-in — F63's class, now contained by the shape as well as absent from the parser.
#[cfg(not(target_arch = "wasm32"))]
async fn catch_callback(listener: tokio::net::TcpListener, secret: &str) -> Result<String, String> {
    let callback_path = format!("/{secret}");
    let mut live: tokio::task::JoinSet<Option<Result<String, String>>> =
        tokio::task::JoinSet::new();

    loop {
        tokio::select! {
            // Disabled at the cap, so a flood queues in the kernel rather than in this process.
            accepted = listener.accept(), if live.len() < CONCURRENT_CONNECTIONS => {
                let (stream, _) = accepted
                    .map_err(|e| format!("the local port stopped accepting: {e}"))?;
                live.spawn(serve(stream, callback_path.clone()));
            }
            // Disabled when empty: `join_next` on an empty set answers `None` at once, which
            // would spin this loop instead of waiting in it.
            finished = live.join_next(), if !live.is_empty() => {
                if let Some(Ok(Some(outcome))) = finished {
                    return outcome;
                }
            }
        }
    }
}

/// One connection, from its first byte to its answer, and whether it ended the sign-in.
///
/// `None` is "keep waiting", and it is the answer to nearly everything: a wrong path, a wrong
/// method, a head that never finished, a peer that never spoke, and a request to the right path
/// carrying nothing. Only a code or a refusal is `Some`.
///
/// **That `None` is a guard rather than a default** — it is the rule the wrong-nonce arm used to
/// carry. A stray local request must not be able to cancel a sign-in still in flight.
#[cfg(not(target_arch = "wasm32"))]
async fn serve(
    mut stream: tokio::net::TcpStream,
    callback_path: String,
) -> Option<Result<String, String>> {
    // A peer that hung up mid-head and a peer that opened and said nothing are the same thing to
    // this listener: answered, and dropped.
    let head = tokio::time::timeout(CONNECTION_DEADLINE, read_request(&mut stream)).await;
    let Ok(Some(request)) = head else {
        respond(&mut stream, "text/plain", "bad request").await;
        return None;
    };

    // The path only — a query is not part of what is being matched, and comparing with one
    // attached would make `?x` a way past the secret.
    let (path, query) = request
        .target
        .split_once('?')
        .unwrap_or((request.target.as_str(), ""));

    if request.method != "GET" || path != callback_path.as_str() {
        // Everything else, including the bare root an attacker would aim at.
        respond(&mut stream, "text/plain", "not found").await;
        return None;
    }

    // The same reader the web build's callback goes through, rather than a second one to keep
    // in step with it — `parse_parameters` is `auth-js`'s own `parseParametersFromURL`.
    let params = crate::cloud::oauth::parse_parameters(&format!("?{query}"));

    if let Some(description) = params.get("error_description").or(params.get("error")) {
        let refusal = format!("the provider refused the sign-in: {description}");
        respond(&mut stream, "text/html", &landing_page(REFUSED)).await;
        return Some(Err(refusal));
    }

    let Some(code) = params.get("code").cloned() else {
        // A request to the right path with nothing on it is not an answer.
        respond(&mut stream, "text/plain", "not found").await;
        return None;
    };

    respond(&mut stream, "text/html", &landing_page(SIGNED_IN)).await;
    Some(Ok(code))
}

/// As much of one HTTP request as this listener needs: the method and the target.
#[cfg(not(target_arch = "wasm32"))]
struct Request {
    method: String,
    target: String,
}

/// Read one request head: the method and the target off the first line.
///
/// Capped at 64 KiB, and the cap is the point rather than tidiness — an unbounded read off a
/// socket anything on this machine may connect to is a memory bomb with a one-line exploit. A real
/// callback is a few hundred bytes.
///
/// **It stops at the blank line and never touches the body, and that is what closes F63.** The
/// version this replaced converted the whole buffer with `String::from_utf8_lossy` and then sliced
/// the resulting `String` at a `Content-Length` taken off the wire. Lossy conversion is not
/// length-preserving — one invalid byte becomes a three-byte U+FFFD — so `Content-Length: 1` with
/// a body of `0xFF` asked for byte 1 of a string whose first character occupies bytes 0..3, and
/// Rust panics on a slice landing inside a character. That ran before any path or secret check, so
/// any web page could reach it with a CORS-simple POST to a guessed port and take the sign-in down
/// with no refusal shown.
///
/// The first fix was to do the offset arithmetic in bytes. This is the second and better one:
/// under PKCE nothing this listener acts on is ever in a body, so there is no body to decode, no
/// offset to compute, and the whole class is absent rather than guarded. Not waiting for a
/// declared body is a small bonus in the same direction — a request that announces 64 KiB and
/// dribbles is answered and dropped instead of held.
#[cfg(not(target_arch = "wasm32"))]
async fn read_request(stream: &mut tokio::net::TcpStream) -> Option<Request> {
    use tokio::io::AsyncReadExt;
    const CAP: usize = 64 * 1024;
    const BLANK_LINE: &[u8] = b"\r\n\r\n";

    let mut raw: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        let read = stream.read(&mut chunk).await.ok()?;
        if read == 0 {
            return None; // the peer hung up mid-request
        }
        raw.extend_from_slice(&chunk[..read]);
        if raw.len() > CAP {
            return None;
        }
        let Some(head_end) = raw
            .windows(BLANK_LINE.len())
            .position(|window| window == BLANK_LINE)
        else {
            continue; // headers not finished
        };
        // One conversion, of a slice whose bounds this side settled, and read only by `lines` and
        // `split` — never indexed at a number that arrived on the socket.
        let head = String::from_utf8_lossy(&raw[..head_end]);
        let mut parts = head.lines().next()?.split(' ');
        return Some(Request {
            method: parts.next()?.to_string(),
            target: parts.next()?.to_string(),
        });
    }
}

/// Answer one request and close, under the same deadline the read is under.
///
/// **The timeout is not belt and braces.** A peer that advertises a zero receive window stalls
/// `write_all` for as long as it cares to, and a stalled write holds its slot exactly as a stalled
/// read once held the whole loop (F64). The result stays discarded, because there is nothing to be
/// done about a peer that will not listen — and discarding it is what lets a caught code be
/// returned rather than lost to a slow reader.
///
/// **This half is reasoned rather than graded.** No test below produces a peer that accepts a
/// connection and then refuses to read, so removing this timeout turns nothing red — unlike the
/// read deadline, which `a_saturated_listener_still_finishes_the_sign_in` fails without.
#[cfg(not(target_arch = "wasm32"))]
async fn respond(stream: &mut tokio::net::TcpStream, content_type: &str, body: &str) {
    use tokio::io::AsyncWriteExt;
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: {content_type}; charset=utf-8\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = tokio::time::timeout(CONNECTION_DEADLINE, async {
        let _ = stream.write_all(response.as_bytes()).await;
        let _ = stream.shutdown().await;
    })
    .await;
}

#[cfg(not(target_arch = "wasm32"))]
const SIGNED_IN: &str = "Signed in. You can close this tab and go back to Sidekick.";
#[cfg(not(target_arch = "wasm32"))]
const REFUSED: &str = "Sign-in was refused. You can close this tab; Sidekick will say why.";

/// The page the browser is left looking at.
///
/// **It is the last thing the provider's redirect touches, and it does nothing.** No script, no
/// fetch, no navigation, and nothing from the query written into it. The page this replaced had
/// all four, because under the implicit flow it was the only thing that could see the fragment —
/// and F04 is the record of how that went: it navigated to a `/callback?<tokens>` URL and put the
/// access and refresh tokens into the address bar and the session history on the way.
///
/// The two messages are constants rather than parameters so that "renders no caller-supplied text"
/// is a property of the signature instead of a habit of the call sites.
#[cfg(not(target_arch = "wasm32"))]
fn landing_page(message: &str) -> String {
    format!(
        "<!doctype html><meta charset=\"utf-8\"><title>Sidekick</title>\
         <body style=\"font:14px system-ui;padding:2rem\">{message}</body>"
    )
}

/// The PKCE verifier's size. 32 bytes is 64 hex characters, inside RFC 7636 §4.1's 43-to-128 —
/// **a constant rather than a literal at the call site because nothing downstream would complain
/// about a shorter one.** GoTrue validates the challenge it is sent and only hashes the verifier,
/// so a verifier cut to 16 bytes would sign in successfully, be out of spec, and have a quarter of
/// the entropy, with every test still green. Named here, it is one number a guard can assert on.
#[cfg(not(target_arch = "wasm32"))]
const VERIFIER_BYTES: usize = 32;

/// The redirect secret's size. 128 bits against a guess at one live path.
#[cfg(not(target_arch = "wasm32"))]
const SECRET_BYTES: usize = 16;

/// `bytes` bytes from the operating system's CSPRNG, hex-encoded.
///
/// Two callers, wanting the same property for different reasons. The redirect secret (16 bytes)
/// is what keeps a guessed port from reaching this listener's one live path. The PKCE verifier
/// (32 bytes, so 64 hex characters — RFC 7636 §4.1 wants 43 to 128, from the unreserved set) is
/// what the authorization code is worthless without.
///
/// **F65: this used to hash the clock through `RandomState`.** That is `DefaultHasher` —
/// SipHash-1-3 with a 64-bit output, which std documents as not cryptographically secure and
/// explicitly free to change between releases. Whatever its seed, a hash whose strength is a
/// documented non-guarantee is the wrong thing under a value an attacker must not be able to
/// guess. `getrandom` is a direct read of the platform CSPRNG and nothing else.
///
/// **It returns `Result` rather than falling back** (Rule 1). A value this flow could not draw
/// randomly is one an attacker may be able to guess, and a weak-but-present one would look exactly
/// as convincing while protecting nothing. The refusal surfaces in the same modal every other
/// sign-in failure uses.
#[cfg(not(target_arch = "wasm32"))]
fn random_hex(bytes: usize) -> Result<String, String> {
    use std::fmt::Write as _;
    let mut drawn = vec![0u8; bytes];
    getrandom::fill(&mut drawn)
        .map_err(|e| format!("could not draw a random value to sign in with: {e}"))?;
    Ok(drawn
        .iter()
        .fold(String::with_capacity(bytes * 2), |mut hex, b| {
            let _ = write!(hex, "{b:02x}");
            hex
        }))
}
