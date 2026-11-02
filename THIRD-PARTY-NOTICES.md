# Third-Party Notices

Morrow is distributed under the **Morrow Unified License** (`LICENSE.md`). That
license covers only material owned by the Project. This file records the
third-party components that are redistributed **alongside** Morrow, as required by
`LICENSE.md` §10 ("Where third-party components are distributed alongside Morrow,
their applicable license information must be preserved").

Nothing here changes, extends, or restricts the terms of `LICENSE.md`; it exists so
the upstream obligations travel with the binaries that carry them.

## 1. Components shipped in the distribution archive

All of these live under `lib/crt/` and are redistributed **unmodified**, exactly as
provided by the toolchain build that produced them.

| Component | Files | Upstream | License | Text |
|---|---|---|---|---|
| GCC runtime libraries | `libgcc.a`, `libgcc_eh.a`, `libgcc_s.a`, `libstdc++.a` | GNU Compiler Collection | GPL-3.0-or-later **with the GCC Runtime Library Exception 3.1** | `licenses/GPL-3.0.txt`, `licenses/GCC-RUNTIME-LIBRARY-EXCEPTION-3.1.txt` |
| mingw-w64 CRT and import libraries | `crt2.o`, `dllcrt2.o`, `libmingw32.a`, `libmingwex.a`, `libmoldname.a`, `libmsvcrt.a`, `libpthread.a`, `libm.a`, and the Windows API import stubs (`libkernel32.a`, `libuser32.a`, `libadvapi32.a`, …) | mingw-w64 | Zope Public License 2.1; parts prominently marked in upstream sources are public domain, BSD, or LGPL | `licenses/ZPL-2.1.txt` |
| GNU libiconv | `libiconv.a` | GNU libiconv | LGPL-2.1-or-later | `licenses/LGPL-2.1.txt` |
| SQLite | `libsqlite3.a` | SQLite | Public domain (no attribution required; credited here voluntarily) | — |

The Windows API import stubs are link-time descriptions of Microsoft's public API
surface. They contain no Microsoft code.

## 2. Components embedded in compiled binaries

`bin/forge.exe` and `bin/mwc.exe` — and any program compiled with Morrow on Windows
using the bundled CRT — statically include portions of the **mingw-w64 runtime** and
of the **GCC runtime** listed above.

- The GCC Runtime Library Exception 3.1 expressly permits distributing such
  compiled binaries under terms of your choosing; it does not extend to Morrow
  itself.
- The mingw-w64 runtime is covered by ZPL 2.1, which permits redistribution in
  binary form provided the copyright and permission notices are retained — this file
  and `licenses/ZPL-2.1.txt` serve that purpose.

## 3. Material in the repository but not in the archive

- `archive/stage0-rust/` — the historical Stage-0 Rust implementation. Its crate
  dependencies are declared in the respective `Cargo.toml` files and are fetched by
  Cargo rather than bundled.
- `benchmarks/rust_tokio/` — benchmark source whose dependencies (`tokio`, `syn`,
  …) are likewise fetched by Cargo, not vendored. Only the sources in this
  repository ship, and they are the Project's own code.

## 4. Provenance

The archives under `lib/crt/` were extracted from a mingw-w64 toolchain build and
have not been modified. The exact toolchain build identifier is not recorded in this
repository; if you need it for a compliance review, ask through the project
repository (`LICENSE.md` §15).

## 5. Maintenance rule

When a vendored binary, a bundled library, or a new runtime dependency enters the
distribution, it must get a row in §1 and, where the upstream license requires it, a
verbatim text under `licenses/`. The release pipeline refuses to package an archive
that does not contain both `LICENSE.md` and this file.
