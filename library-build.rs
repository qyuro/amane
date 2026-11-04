use std::fs;
use std::path::Path;

/*
 * Local fixes for the lock screen.
 *
 * amane's library creates its ext-session-lock surfaces only after the
 * compositor sends `locked()`, but niri (like the protocol recommends) does
 * not send `locked()` until the surfaces exist or a 1000 ms deadline expires.
 * Niri therefore waits the full second and shows its red locked background.
 * The surfaces may be created immediately after `lock()`, so the first patch
 * below removes that wait.
 *
 * The second patch makes Lock::unlocked() public, so the shell can tell a
 * fresh lock from the idle timer asking again while the screen is already up,
 * and only plays the appearance animation once per lock.
 *
 * These files are not part of the embedded library, so `unpack` leaves this
 * script in place and Cargo reruns it whenever the sources are restored.
 */

const GUARD: &str = "        // monitors described before the lock is granted get their screens in locked()
        if !session_lock.is_locked() {
            return;
        }

";

const ASSIGN: &str = "        self.session_lock = Some(session_lock);
";

const OPEN: &str = "        self.session_lock = Some(session_lock);

        // ext-session-lock asks for the surfaces right away; waiting for locked()
        // makes niri sit on its red background for a full second
        let outputs: Vec<WlOutput> = self.output.outputs().collect();

        for output in outputs {
            self.open_lock(output);
        }
";

const UNLOCKED: &str = "    pub(crate) fn unlocked(&self) -> bool {";

const UNLOCKED_PUBLIC: &str = "    pub fn unlocked(&self) -> bool {";

fn main() {
    println!("cargo:rerun-if-changed=src/wayland/lock.rs");
    println!("cargo:rerun-if-changed=src/services/lock.rs");

    patch_surfaces();
    patch_unlocked();
}

fn patch_surfaces() {
    let path = Path::new("src/wayland/lock.rs");

    let Ok(source) = fs::read_to_string(path) else {
        println!("cargo:warning=amane-lock-fix: src/wayland/lock.rs is missing");

        return;
    };

    if source.contains("ext-session-lock asks for the surfaces right away") {
        return;
    }

    if !source.contains(GUARD) || !source.contains(ASSIGN) {
        println!("cargo:warning=amane-lock-fix: lock.rs changed, the early-surface patch was not applied");

        return;
    }

    let patched = source.replacen(GUARD, "", 1).replacen(ASSIGN, OPEN, 1);

    if let Err(error) = fs::write(path, patched) {
        println!("cargo:warning=amane-lock-fix: could not patch lock.rs: {error}");
    }
}

fn patch_unlocked() {
    let path = Path::new("src/services/lock.rs");

    let Ok(source) = fs::read_to_string(path) else {
        println!("cargo:warning=amane-lock-fix: src/services/lock.rs is missing");

        return;
    };

    if source.contains(UNLOCKED_PUBLIC) {
        return;
    }

    if !source.contains(UNLOCKED) {
        println!("cargo:warning=amane-lock-fix: services/lock.rs changed, the unlocked patch was not applied");

        return;
    }

    let patched = source.replacen(UNLOCKED, UNLOCKED_PUBLIC, 1);

    if let Err(error) = fs::write(path, patched) {
        println!("cargo:warning=amane-lock-fix: could not patch services/lock.rs: {error}");
    }
}
