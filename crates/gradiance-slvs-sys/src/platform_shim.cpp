// The four `SolveSpace::Platform::` symbols the solver needs, supplied here
// instead of by upstream's `src/platform/platformbase.cpp`.
//
// That file is not vendored, for one reason: it pulls in mimalloc — roughly
// 14k lines of allocator — and uses it for exactly one thing, a bump arena
// behind AllocTemporary/FreeAllTemporary. The expression allocator grabs
// temporaries while building equations and then discards the lot in one go,
// which a chunk list satisfies just as well at a scale (a sketch, not a solid
// model) where allocator throughput is not the bottleneck.
//
// This lives on Gradiance's side of the line rather than as a patch to
// `third_party/solvespace/`, which is what keeps that tree byte-identical to
// upstream — see third_party/solvespace/SOURCE.md.

#include <cstdarg>
#include <cstdio>
#include <cstdlib>
#include <vector>

#include "util.h"
#include "platform/platform.h"

namespace SolveSpace {
namespace Platform {

void DebugPrint(const char *fmt, ...) {
    va_list va;
    va_start(va, fmt);
    vfprintf(stderr, fmt, va);
    fputc('\n', stderr);
    va_end(va);
}

// Thread-local to match upstream: the solver's temporaries never cross threads,
// and a per-thread arena keeps FreeAllTemporary from touching another thread's
// allocations.
static thread_local std::vector<void *> TempArena;

void *AllocTemporary(size_t size) {
    // Zeroing matches mi_heap_zalloc, which upstream relies on: Expr nodes are
    // allocated here and read before every field is assigned.
    void *ptr = calloc(1, size);
    ssassert(ptr != NULL, "out of memory");
    TempArena.push_back(ptr);
    return ptr;
}

void FreeAllTemporary() {
    for(void *ptr : TempArena) {
        free(ptr);
    }
    TempArena.clear();
    // Release the bookkeeping vector too, so a big solve does not leave its
    // high-water mark resident for the life of the thread.
    TempArena.shrink_to_fit();
}

} // namespace Platform
} // namespace SolveSpace
