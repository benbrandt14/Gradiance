// Compile-time proof that `src/ffi.rs` describes the same memory as `slvs.h`.
//
// The FFI declarations are hand-written, so nothing generates them from the
// header and nothing would otherwise notice if an upstream bump reordered a
// struct. A wrong offset would not fail to link — it would silently feed the
// solver garbage, which is the worst failure mode available.
//
// So both sides are pinned to one table of literal numbers: these
// static_asserts check the numbers against the real C header, and
// `layout_matches_the_c_header` in lib.rs checks the identical numbers against
// the Rust structs. Drift on either side breaks the build.

#include <cstddef>
#include "slvs.h"

#define CHECK_SIZE(type, bytes) \
    static_assert(sizeof(type) == (bytes), #type " changed size; update src/ffi.rs")
#define CHECK_OFFSET(type, field, bytes) \
    static_assert(offsetof(type, field) == (bytes), \
                  #type "." #field " moved; update src/ffi.rs")

CHECK_SIZE(Slvs_Param, 16);
CHECK_OFFSET(Slvs_Param, h, 0);
CHECK_OFFSET(Slvs_Param, group, 4);
CHECK_OFFSET(Slvs_Param, val, 8);

CHECK_SIZE(Slvs_Entity, 56);
CHECK_OFFSET(Slvs_Entity, h, 0);
CHECK_OFFSET(Slvs_Entity, group, 4);
CHECK_OFFSET(Slvs_Entity, type, 8);
CHECK_OFFSET(Slvs_Entity, wrkpl, 12);
CHECK_OFFSET(Slvs_Entity, point, 16);
CHECK_OFFSET(Slvs_Entity, normal, 32);
CHECK_OFFSET(Slvs_Entity, distance, 36);
CHECK_OFFSET(Slvs_Entity, param, 40);

CHECK_SIZE(Slvs_Constraint, 56);
CHECK_OFFSET(Slvs_Constraint, h, 0);
CHECK_OFFSET(Slvs_Constraint, group, 4);
CHECK_OFFSET(Slvs_Constraint, type, 8);
CHECK_OFFSET(Slvs_Constraint, wrkpl, 12);
CHECK_OFFSET(Slvs_Constraint, valA, 16);
CHECK_OFFSET(Slvs_Constraint, ptA, 24);
CHECK_OFFSET(Slvs_Constraint, ptB, 28);
CHECK_OFFSET(Slvs_Constraint, entityA, 32);
CHECK_OFFSET(Slvs_Constraint, entityB, 36);
CHECK_OFFSET(Slvs_Constraint, entityC, 40);
CHECK_OFFSET(Slvs_Constraint, entityD, 44);
CHECK_OFFSET(Slvs_Constraint, other, 48);
CHECK_OFFSET(Slvs_Constraint, other2, 52);

// Slvs_System interleaves pointers and ints, so its offsets are pointer-width
// dependent. The numbers below are the 64-bit layout; on a 32-bit target the
// field *order* is what matters and both compilers derive it identically from
// the same declaration order.
#if defined(__SIZEOF_POINTER__) && __SIZEOF_POINTER__ == 8
CHECK_SIZE(Slvs_System, 88);
CHECK_OFFSET(Slvs_System, param, 0);
CHECK_OFFSET(Slvs_System, params, 8);
CHECK_OFFSET(Slvs_System, entity, 16);
CHECK_OFFSET(Slvs_System, entities, 24);
CHECK_OFFSET(Slvs_System, constraint, 32);
CHECK_OFFSET(Slvs_System, constraints, 40);
CHECK_OFFSET(Slvs_System, dragged, 48);
CHECK_OFFSET(Slvs_System, ndragged, 56);
CHECK_OFFSET(Slvs_System, calculateFaileds, 60);
CHECK_OFFSET(Slvs_System, failed, 64);
CHECK_OFFSET(Slvs_System, faileds, 72);
CHECK_OFFSET(Slvs_System, dof, 76);
CHECK_OFFSET(Slvs_System, result, 80);
#endif
