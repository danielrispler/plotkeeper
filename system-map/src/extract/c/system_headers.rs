//! The system-header list: the names an angle-bracket include gives nothing for, so that a tracked file sharing a
//! system header's name (bitcoin's `src/util/string.h`) never catches `<string.h>` (spec #806, "The system-header
//! list"). It holds exactly the lists the research measured with (`docs/studio/research/c-cpp-extractor.md`, "Round 2
//! method": `STD`, `PLATFORM`, `SYS_PREFIX`; "Round 4 method": `WIN`), so the trials' zero wrong lines carry over.

/// The C (C23) and C++ standard headers and the POSIX headers, whitespace-separated. Sources: cppreference, "C Standard
/// Library header files" (https://en.cppreference.com/w/c/header) and "C++ Standard Library headers"
/// (https://en.cppreference.com/w/cpp/header), with the `c`-prefixed compatibility headers; the POSIX.1-2024 "Headers"
/// index (https://pubs.opengroup.org/onlinepubs/9799919799/idx/head.html).
const STD: &str = "assert.h complex.h ctype.h errno.h fenv.h float.h inttypes.h iso646.h limits.h
locale.h math.h setjmp.h signal.h stdalign.h stdarg.h stdatomic.h stdbit.h stdbool.h stdckdint.h
stddef.h stdint.h stdio.h stdlib.h stdnoreturn.h string.h tgmath.h threads.h time.h uchar.h
wchar.h wctype.h aio.h arpa/inet.h cpio.h devctl.h dirent.h dlfcn.h endian.h fcntl.h fmtmsg.h
fnmatch.h ftw.h glob.h grp.h iconv.h langinfo.h libintl.h libgen.h monetary.h mqueue.h ndbm.h
net/if.h netdb.h netinet/in.h netinet/tcp.h nl_types.h poll.h pthread.h pwd.h regex.h sched.h
search.h semaphore.h spawn.h strings.h syslog.h tar.h termios.h unistd.h utmpx.h wordexp.h
algorithm any array atomic barrier bit bitset cassert ccomplex cctype cerrno cfenv cfloat charconv
chrono cinttypes ciso646 climits clocale cmath codecvt compare complex concepts condition_variable
contracts coroutine csetjmp csignal cstdalign cstdarg cstdbool cstddef cstdint cstdio cstdlib
cstring ctgmath ctime cuchar cwchar cwctype debugging deque exception execution expected
filesystem flat_map flat_set format forward_list fstream functional future generator
hazard_pointer hive initializer_list inplace_vector iomanip ios iosfwd iostream istream iterator
latch limits linalg list locale map mdspan memory memory_resource meta mutex new numbers numeric
optional ostream print queue random ranges ratio rcu regex scoped_allocator semaphore set
shared_mutex simd source_location span spanstream sstream stack stacktrace stdexcept stdfloat
stop_token streambuf string string_view strstream syncstream system_error text_encoding thread
tuple type_traits typeindex typeinfo unordered_map unordered_set utility valarray variant vector
version";

/// Common platform headers outside those standards: the glibc manual (`byteswap.h`, `malloc.h`, `alloca.h`,
/// `execinfo.h`, `features.h`), GCC's and Arm's intrinsics documentation (`cpuid.h`, `intrin.h`, `immintrin.h`,
/// `x86intrin.h`, `emmintrin.h`, `arm_neon.h`) and Microsoft Learn (`windows.h`, `winsock2.h`, `ws2tcpip.h`, `io.h`).
const PLATFORM: &str =
    "byteswap.h malloc.h alloca.h execinfo.h features.h cpuid.h windows.h winsock2.h
ws2tcpip.h io.h intrin.h immintrin.h x86intrin.h emmintrin.h arm_neon.h";

/// Paths starting with these name a system folder's headers (glibc and the BSD and Darwin systems).
const SYS_PREFIX: [&str; 8] = [
    "sys/", "linux/", "mach/", "machine/", "bits/", "netinet/", "arpa/", "net/",
];

/// Windows CRT and SDK headers, plus `memory.h`: Microsoft Learn, "C runtime library reference" and the Windows SDK
/// headers.
const WIN: &str =
    "process.h schannel.h windows.h winsock2.h ws2tcpip.h io.h direct.h malloc.h conio.h
tchar.h memory.h";

/// Whether `path`, written in angle brackets, is a system header.
pub(super) fn is_system(path: &str) -> bool {
    SYS_PREFIX.iter().any(|p| path.starts_with(p))
        || [STD, PLATFORM, WIN]
            .iter()
            .any(|list| list.split_whitespace().any(|name| name == path))
}
