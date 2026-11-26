//! A crash in native code on Linux or a Mac (a bad memory access, a stack overflow among them; a
//! bus error; an illegal instruction; an arithmetic fault; an abort) written into the log as the
//! process goes: its signal, where it happened, on which thread, and the frames that led there,
//! each with the file its code is in and how far into it. Then the signal's own action is put back
//! and the signal raised again, so the system's crash report (or core dump) follows as before.
//!
//! The handler runs on a stack of its own, as an overflowed stack has no room left: the thread
//! that starts watching (the window's) is given a large one here, and the app's other threads
//! have the one Rust gives each. It may not allocate or take a lock (the crash may have happened
//! holding one), so it writes with `write` to a descriptor of the log's file taken beforehand, its
//! lines made in buffers of its own (`CrashLine`). What it calls (`dladdr`, the unwinder) is
//! called once as watching starts, so nothing is loaded or looked up for the first time in it.

#![allow(unsafe_code)]

use std::ffi::{CStr, c_int, c_void};
use std::os::fd::IntoRawFd;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicUsize, Ordering};

use procraft_core::{CrashLine, RUN_CRASHED};

/// The log's file, as a descriptor of the handler's own (the log's `File` is let go before the app
/// starts itself again); -1 before [`watch`].
static LOG: AtomicI32 = AtomicI32::new(-1);
/// The thread writing a crash's record (its `pthread_self`), 0 before one crashes.
static WRITING: AtomicUsize = AtomicUsize::new(0);
/// The record is written.
static WRITTEN: AtomicBool = AtomicBool::new(false);

/// A signal handler as `sigaction` takes it, with `SA_SIGINFO`.
type Handler = extern "C" fn(c_int, *mut libc::siginfo_t, *mut c_void);

/// The signals that end the app with a crash.
const SIGNALS: [c_int; 5] = [libc::SIGSEGV, libc::SIGBUS, libc::SIGILL, libc::SIGFPE, libc::SIGABRT];
/// How many frames are written at most, and how many are read (the handler's own among them).
const FRAMES: usize = 64;
const READ: usize = FRAMES + 16;
/// The stack the thread that starts watching runs the handler on: room for it and the unwinder,
/// whatever the processor's state takes of it (a few KB with AVX-512 or AMX).
const STACK: usize = 256 << 10;
/// How far below the stack pointer a function's frame may reach: a bad access that near it is
/// the stack's end.
const OVERFLOW_REACH: usize = 256 << 10;
/// How long a thread that crashes while another writes its record waits for that record, in
/// steps of 10 ms.
const WAIT_STEPS: u32 = 200;
/// How long the record may take, seconds.
const RECORD_SECONDS: libc::c_uint = 10;

/// Crashes written into the log's file `log` from now on (`log` a descriptor of its own, kept
/// open), the calling thread given a large stack to handle them on.
pub fn watch(log: std::fs::File) {
    LOG.store(log.into_raw_fd(), Ordering::SeqCst);
    warm_up();
    stack_for_this_thread();
    handle();
}

/// Puts the handler back if something has put its own in its place (a driver's runtime, as it
/// loads): only the last one set is called.
pub fn watch_again() {
    if LOG.load(Ordering::SeqCst) < 0 {
        return;
    }
    let ours = handler as Handler as usize;
    let replaced = SIGNALS.iter().any(|&signal| {
        // SAFETY: zeroed, to be filled in
        let mut now: libc::sigaction = unsafe { std::mem::zeroed() };
        // SAFETY: only reads the signal's action
        unsafe { libc::sigaction(signal, std::ptr::null(), &mut now) == 0 && now.sa_sigaction != ours }
    });
    if replaced {
        handle();
        log::warn!("something had replaced the app's crash handler: put back");
    }
}

/// The handler set for each of the signals, on the alternate stack.
fn handle() {
    for signal in SIGNALS {
        // SAFETY: zeroed, then filled in as sigaction(2) takes it
        let mut action: libc::sigaction = unsafe { std::mem::zeroed() };
        action.sa_sigaction = handler as Handler as usize;
        action.sa_flags = libc::SA_SIGINFO | libc::SA_ONSTACK;
        // SAFETY: an empty mask; the handler does only what a signal handler may
        let set = unsafe {
            libc::sigemptyset(&mut action.sa_mask);
            libc::sigaction(signal, &action, std::ptr::null_mut())
        };
        if set != 0 {
            log::warn!("the crash handler for signal {signal}: {}", std::io::Error::last_os_error());
        }
    }
}

/// The calling thread given a stack of [`STACK`] to handle signals on, an unreadable page below it
/// (a handler that runs out of it stops there, rather than writing over what is below).
fn stack_for_this_thread() {
    // SAFETY: only reads the system's page size
    let page = usize::try_from(unsafe { libc::sysconf(libc::_SC_PAGESIZE) }).unwrap_or(4096).max(4096);
    let size = STACK + page;
    let (read_write, private) = (libc::PROT_READ | libc::PROT_WRITE, libc::MAP_PRIVATE | libc::MAP_ANONYMOUS);
    // SAFETY: a new mapping of the app's own; never unmapped, as the thread may use it until the
    // process ends
    let mapped = unsafe { libc::mmap(std::ptr::null_mut(), size, read_write, private, -1, 0) };
    if mapped == libc::MAP_FAILED {
        log::warn!("no stack for the crash handler: {}", std::io::Error::last_os_error());
        return;
    }
    // SAFETY: the mapping's own first page
    unsafe { libc::mprotect(mapped, page, libc::PROT_NONE) };
    // SAFETY: zeroed, then filled in as sigaltstack(2) takes it
    let mut stack: libc::stack_t = unsafe { std::mem::zeroed() };
    stack.ss_sp = mapped.cast::<u8>().wrapping_add(page).cast();
    stack.ss_size = STACK;
    stack.ss_flags = 0;
    // SAFETY: the stack is the thread's for good
    if unsafe { libc::sigaltstack(&stack, std::ptr::null_mut()) } != 0 {
        log::warn!("the crash handler's stack: {}", std::io::Error::last_os_error());
        // SAFETY: the mapping made above, not in use
        unsafe { libc::munmap(mapped, size) };
    }
}

/// What the handler calls, called once: a library loaded or a symbol looked up as it is first
/// called would allocate in the handler.
fn warm_up() {
    let mut frames = [0usize; READ];
    let _ = read_frames(&Place::default(), &mut frames);
    let mut name = [0u8; 64];
    let _ = thread_name(&mut name);
    let mut line = CrashLine::new();
    let here = warm_up as fn() as usize;
    module(&mut line, here, here);
}

/// What the system calls on a crash's signal: the crash written into the log (by the first thread
/// to crash; one crashing after it waits a while for that record), then the signal's own action
/// put back and the signal raised again, for the system's crash report.
extern "C" fn handler(signal: c_int, info: *mut libc::siginfo_t, context: *mut c_void) {
    // SAFETY: the calling thread's own handle
    let me = unsafe { libc::pthread_self() } as usize;
    match WRITING.compare_exchange(0, me, Ordering::SeqCst, Ordering::SeqCst) {
        Ok(_) => {
            // a lock the crash left held (the loader's, the unwinder's) mustn't keep the process
            // from ending: if the record takes that long, the alarm's signal ends it
            // SAFETY: alarm may be called in a signal handler
            unsafe { libc::alarm(RECORD_SECONDS) };
            write_crash(signal, info, context);
            WRITTEN.store(true, Ordering::SeqCst);
            // SAFETY: as above
            unsafe { libc::alarm(0) };
        }
        Err(writer) if writer != me => wait_for_record(),
        // a crash as this thread wrote its record: on to the system
        Err(_) => {}
    }
    // SAFETY: both may be called in a signal handler; as it returns, a fault's instruction runs
    // again (and faults again) or the signal raised is taken, with the system's own action
    unsafe {
        libc::signal(signal, libc::SIG_DFL);
        libc::raise(signal);
    }
}

/// Waits a while for another thread's record of its crash to be written.
fn wait_for_record() {
    // SAFETY: zeroed, then filled in
    let mut pause: libc::timespec = unsafe { std::mem::zeroed() };
    pause.tv_nsec = 10_000_000;
    for _ in 0..WAIT_STEPS {
        if WRITTEN.load(Ordering::SeqCst) {
            return;
        }
        // SAFETY: nanosleep may be called in a signal handler
        unsafe { libc::nanosleep(&pause, std::ptr::null_mut()) };
    }
}

/// The crash's record, written into the log: its signal, where, on which thread; its frames.
fn write_crash(signal: c_int, info: *mut libc::siginfo_t, context: *mut c_void) {
    let fd = LOG.load(Ordering::SeqCst);
    if fd < 0 {
        return;
    }
    let ms = super::since_launch();
    let at = Place::of(context);
    // SAFETY: the system's information on the signal, valid while the handler runs
    let (code, address) = match unsafe { info.as_ref() } {
        // SAFETY: as above
        Some(info) => (info.si_code, unsafe { info.si_addr() } as usize),
        None => (0, 0),
    };
    let mut line = CrashLine::new();
    line.begin(ms).text(RUN_CRASHED).text("signal ").signed(signal.into());
    line.text(" (").text(name(signal)).text(", ").text(meaning(signal)).text(")");
    if signal == libc::SIGSEGV || signal == libc::SIGBUS {
        line.text(" at address ").hex(address as u64, 0);
        if address >= OVERFLOW_REACH && at.sp != 0 && address.abs_diff(at.sp) <= OVERFLOW_REACH {
            line.text(", next to the stack's end: a stack overflow");
        }
    } else if signal == libc::SIGILL || signal == libc::SIGFPE {
        line.text(" at ").hex(address as u64, 0);
    }
    line.text(", code ").signed(code.into());
    let mut name = [0u8; 64];
    let thread = thread_name(&mut name);
    if !thread.is_empty() {
        line.text(", on thread \"").bytes(thread).text("\"");
    }
    write_all(fd, line.ended());
    write_all(fd, line.begin(ms).text("backtrace, the innermost frame first:").ended());
    let mut frames = [0usize; READ];
    let count = read_frames(&at, &mut frames);
    for (index, &pc) in frames.iter().take(count.min(FRAMES)).enumerate() {
        line.begin(ms).frame(index, pc as u64);
        // a return address is the instruction after its call, which may be past the calling
        // function's end: the call's own file and symbol are looked up
        module(&mut line, if index == 0 { pc } else { pc.saturating_sub(1) }, pc);
        write_all(fd, line.ended());
    }
}

/// The file whose code is at `pc` and how far into it, as `dladdr` tells of `lookup`, with the
/// symbol there if the file names one.
fn module(line: &mut CrashLine, lookup: usize, pc: usize) {
    // SAFETY: zeroed, to be filled in
    let mut info: libc::Dl_info = unsafe { std::mem::zeroed() };
    // SAFETY: the address is only looked up in the loader's list, not read
    let found = unsafe { libc::dladdr(std::ptr::without_provenance(lookup), &mut info) } != 0;
    if !found || info.dli_fname.is_null() {
        line.text(" (in no file's code)");
        return;
    }
    // SAFETY: the loader's own text, there while the file stays loaded
    let file = unsafe { CStr::from_ptr(info.dli_fname) }.to_bytes();
    line.module(file, pc.wrapping_sub(info.dli_fbase as usize) as u64);
    if !info.dli_sname.is_null() {
        // SAFETY: as above
        let symbol = unsafe { CStr::from_ptr(info.dli_sname) }.to_bytes();
        line.text(" (").bytes(symbol).text("+").hex(pc.wrapping_sub(info.dli_saddr as usize) as u64, 0).text(")");
    }
}

/// Writes all of `bytes` to `fd`, as far as it takes them.
fn write_all(fd: c_int, mut bytes: &[u8]) {
    while !bytes.is_empty() {
        // SAFETY: from the bytes given, their length given
        let written = unsafe { libc::write(fd, bytes.as_ptr().cast(), bytes.len()) };
        match usize::try_from(written) {
            Ok(0) => return,
            Ok(n) => bytes = bytes.get(n..).unwrap_or_default(),
            Err(_) if std::io::Error::last_os_error().raw_os_error() == Some(libc::EINTR) => {}
            Err(_) => return,
        }
    }
}

fn name(signal: c_int) -> &'static str {
    match signal {
        libc::SIGSEGV => "SIGSEGV",
        libc::SIGBUS => "SIGBUS",
        libc::SIGILL => "SIGILL",
        libc::SIGFPE => "SIGFPE",
        libc::SIGABRT => "SIGABRT",
        _ => "another signal",
    }
}

fn meaning(signal: c_int) -> &'static str {
    match signal {
        libc::SIGSEGV => "a bad memory access",
        libc::SIGBUS => "a bus error",
        libc::SIGILL => "an illegal instruction",
        libc::SIGFPE => "an arithmetic fault",
        libc::SIGABRT => "an abort",
        _ => "a crash",
    }
}

/// The calling thread's name, read into `buffer`; empty if it has none.
#[cfg(target_os = "linux")]
fn thread_name(buffer: &mut [u8; 64]) -> &[u8] {
    // SAFETY: the kernel writes the thread's name and its end, 16 bytes at most, into the buffer
    if unsafe { libc::prctl(libc::PR_GET_NAME, buffer.as_mut_ptr()) } != 0 {
        return &[];
    }
    until_end(buffer)
}

#[cfg(target_os = "macos")]
fn thread_name(buffer: &mut [u8; 64]) -> &[u8] {
    // SAFETY: fills the buffer, its length given, with the thread's name and its end
    if unsafe { libc::pthread_getname_np(libc::pthread_self(), buffer.as_mut_ptr().cast(), buffer.len()) } != 0 {
        return &[];
    }
    until_end(buffer)
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn thread_name(_buffer: &mut [u8; 64]) -> &[u8] {
    &[]
}

/// A C string's bytes, up to its end.
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn until_end(buffer: &[u8]) -> &[u8] {
    let end = buffer.iter().position(|&b| b == 0).unwrap_or(buffer.len());
    buffer.get(..end).unwrap_or_default()
}

/// Where the crashed thread was as the signal came: its instruction, stack, frame and link
/// registers (0 where not known).
#[derive(Clone, Copy, Default)]
struct Place {
    pc: usize,
    sp: usize,
    // (walked from on a Mac; elsewhere the system's unwinder reads the frames)
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    fp: usize,
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    lr: usize,
}

impl Place {
    /// From the thread's context as the system hands it to the handler.
    fn of(context: *mut c_void) -> Place {
        // SAFETY: the system's record of the thread as the signal came, valid while the handler runs
        match unsafe { context.cast::<libc::ucontext_t>().as_ref() } {
            Some(context) => Place::read(context),
            None => Place::default(),
        }
    }

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    fn read(context: &libc::ucontext_t) -> Place {
        let register = |r: c_int| {
            let value = usize::try_from(r).ok().and_then(|r| context.uc_mcontext.gregs.get(r));
            value.map_or(0, |&v| v as usize)
        };
        Place { pc: register(libc::REG_RIP), sp: register(libc::REG_RSP), fp: register(libc::REG_RBP), lr: 0 }
    }

    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    fn read(context: &libc::ucontext_t) -> Place {
        let m = &context.uc_mcontext;
        let register = |r: usize| m.regs.get(r).map_or(0, |&v| v as usize);
        Place { pc: m.pc as usize, sp: m.sp as usize, fp: register(29), lr: register(30) }
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    fn read(context: &libc::ucontext_t) -> Place {
        // SAFETY: the thread's machine state the context points to, valid as the context is
        let Some(m) = (unsafe { context.uc_mcontext.as_ref() }) else { return Place::default() };
        let s = &m.__ss;
        Place { pc: s.__pc as usize, sp: s.__sp as usize, fp: s.__fp as usize, lr: s.__lr as usize }
    }

    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    fn read(context: &libc::ucontext_t) -> Place {
        // SAFETY: the thread's machine state the context points to, valid as the context is
        let Some(m) = (unsafe { context.uc_mcontext.as_ref() }) else { return Place::default() };
        let s = &m.__ss;
        Place { pc: s.__rip as usize, sp: s.__rsp as usize, fp: s.__rbp as usize, lr: 0 }
    }

    #[cfg(not(any(
        all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64")),
        all(target_os = "macos", any(target_arch = "x86_64", target_arch = "aarch64")),
    )))]
    fn read(_context: &libc::ucontext_t) -> Place {
        Place::default()
    }
}

/// Puts `pc` at `out[*count]`, if there is room.
fn put(out: &mut [usize], count: &mut usize, pc: usize) {
    if let Some(slot) = out.get_mut(*count) {
        *slot = pc;
        *count += 1;
    }
}

/// The crashed thread's frames, innermost first, into `out`; how many. Read by the system's
/// unwinder from the handler (through the signal's own frame, into the thread's), from the frame
/// the crash happened in: the handler's own left out. If the unwinder doesn't reach it, that
/// frame comes first, then what it read.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn read_frames(at: &Place, out: &mut [usize; READ]) -> usize {
    let most = c_int::try_from(READ).unwrap_or(0);
    // SAFETY: fills at most `most` addresses into `out` (an address is a usize, as a pointer is)
    let count = usize::try_from(unsafe { libc::backtrace(out.as_mut_ptr().cast(), most) }).unwrap_or(0).min(READ);
    if at.pc == 0 {
        return count;
    }
    match out.iter().take(count).position(|&pc| pc == at.pc) {
        Some(from) => {
            out.copy_within(from..count, 0);
            count - from
        }
        None => {
            out.copy_within(0..count.min(READ - 1), 1);
            let mut first = 0;
            put(out, &mut first, at.pc);
            (count + 1).min(READ)
        }
    }
}

/// The crashed thread's frames, innermost first, into `out`; how many. Read down the chain of
/// frame records from where the crash happened (a Mac's code always keeps it), each inside the
/// thread's stack, or the walk stops.
#[cfg(target_os = "macos")]
fn read_frames(at: &Place, out: &mut [usize; READ]) -> usize {
    /// What an address keeps of a return address (one the processor signed has its signature in
    /// the bits above).
    const ADDRESS: usize = 0x0000_7fff_ffff_ffff;
    // SAFETY: the calling thread's own stack, as the system keeps it
    let (top, size) = unsafe {
        let me = libc::pthread_self();
        (libc::pthread_get_stackaddr_np(me) as usize, libc::pthread_get_stacksize_np(me))
    };
    let bottom = top.saturating_sub(size);
    let readable = |fp: usize| fp.is_multiple_of(8) && fp >= bottom && fp.saturating_add(16) <= top;
    // SAFETY: (where called) inside the thread's stack: a frame record, the frame before's
    // address and the return address
    let record = |fp: usize| unsafe {
        let words = std::ptr::with_exposed_provenance::<usize>(fp);
        (words.read(), words.add(1).read())
    };
    if at.pc == 0 {
        return 0;
    }
    let mut count = 0;
    put(out, &mut count, at.pc);
    let mut fp = at.fp;
    // the caller of a function that hasn't made its frame (yet) is in the link register
    let first = if readable(fp) { record(fp).1 & ADDRESS } else { 0 };
    if at.lr != 0 && at.lr & ADDRESS != first {
        put(out, &mut count, at.lr & ADDRESS);
    }
    while count < READ && readable(fp) {
        let (next, back) = record(fp);
        if back == 0 {
            break;
        }
        put(out, &mut count, back & ADDRESS);
        if next <= fp {
            break;
        }
        fp = next;
    }
    count
}

/// The crashed thread's frame alone, where the frames can't be read.
#[cfg(not(any(target_os = "macos", all(target_os = "linux", target_env = "gnu"))))]
fn read_frames(at: &Place, out: &mut [usize; READ]) -> usize {
    let mut count = 0;
    if at.pc != 0 {
        put(out, &mut count, at.pc);
    }
    count
}
