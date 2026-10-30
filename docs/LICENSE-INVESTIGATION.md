# LICENSE INVESTIGATION — C35 and DeBrief

Research only. No tree edits, no repo writes, no license changes made.
Written 2026-10-30. **This is research, not legal advice.**

## 0. The question

Chase states the game and the engines already have AGPL licenses, and asks
what alternative licenses to recommend. The provisional recommendation under
test was: C35 engine → GPLv3, DeBrief game → stay AGPL, with MPL-2.0 as the
alternative if closed mod plugins matter more than DarkPlaces reuse.
Verdict, up front: **the provisional recommendation is CONFIRMED, with one
correction** — the DarkPlaces door is open under AGPLv3 too, so the GPLv3
switch is justified by adoption and simplicity, NOT by unlocking DarkPlaces
(see §1.3 and §4).

## 1. Verified facts (from the actual repositories, not memory)

### 1.1 nightwarelabz/C35 — AGPLv3-only

- `LICENSE` is the full GNU Affero General Public License, Version 3,
  19 November 2007 (blob `0ad25db4bd1d86c452db3f9602ccdbe172438f52`,
  fetched from main).
- `README.md` declares "AGPL-3.0 — see LICENSE".
- `crates/c35/Cargo.toml` declares `license = "AGPL-3.0-only"` (SPDX).
- So the operative declaration is **AGPLv3-only**, not "or later".
  Consequence: a future AGPLv4 cannot be adopted by downstream on its own;
  only the copyright holder can move the project to a new version.
- The `LICENSE` file itself is the bare FSF text: it carries no copyright
  attribution line naming Chase and no per-program "or later" notice. The
  project's license posture therefore rests on the README line and the
  Cargo manifest, which should be tidied at any relicense (see §5).

### 1.2 nightwarelabz/DeBrief — AGPLv3

- `LICENSE` is byte-identical to C35's (same blob
  `0ad25db4bd1d86c452db3f9602ccdbe172438f52`).
- `README.md` declares "AGPL-3.0 — see LICENSE".
- No separate asset license was found: the `concept-art/` images sit in
  the same repo under the same LICENSE file. Whether the art is meant to
  be AGPL-licensed is currently ambiguous — an open decision, flagged
  in §4.3, not assumed either way.

### 1.3 DarkPlaces — GPL-2.0-or-later (the "or later" clause is real)

- `COPYING` in DarkPlacesEngine/darkplaces is the GPL version 2 text
  (Version 2, June 1991).
- The per-file headers decide the operative license, and they say it
  explicitly, across multiple files (`darkplaces.h`, `vpk.h`, `wad.h`,
  `mdfour.h`, `model_sprite.h`, `image_png.h`):
  > "you can redistribute it and/or modify it under the terms of the GNU
  > General Public License as published by the Free Software Foundation;
  > either version 2 of the License, or (at your option) any later
  > version."
- Debian's copyright file for the package records "License: GPL-2+".
- **DarkPlaces is therefore GPL-2.0-or-later (GPL-2.0+).** Two consequences
  follow, both verified against the license texts themselves:
  1. DarkPlaces code may be used **under GPLv3 terms** (the "or later"
     clause makes GPLv3 one of the permitted options; the FSF's own FAQ
     states GPLv2-or-later code is compatible with GPLv3 for exactly
     this reason).
  2. DarkPlaces code may be combined with an **AGPLv3** project: AGPLv3
     §13 (verified in the repos' own LICENSE text) expressly permits
     linking or combining an AGPL-covered work with a work licensed
     under GPLv3, with each part remaining under its own license.
- The DarkPlaces door is therefore open under BOTH current AGPLv3 and
  proposed GPLv3. Any argument that C35 must switch to GPLv3 "to use
  DarkPlaces" is wrong, and this investigation corrects it.

### 1.4 Contributors — sole author in the sampled history

- Last 30 commits of each repo, by GitHub login: only `nightwarelabz`
  (the human account) and `web-flow` (GitHub's merge bot, not a
  copyright holder). No outside human contributor appears in the sample.
- Caveat, stated plainly: this is a 30-commit sample, not a full-history
  audit. Before any relicense, audit the complete commit and PR history.
  Note also that substantial code was written by AI agents under Chase's
  direction and committed under his account; no separate copyright holder
  appears in the record, but that is a fact about the record, not a
  legal conclusion.
- Practical meaning: relicensing is clean **while** sole authorship holds.
  The moment an outside contribution is merged without a contributor
  agreement, relicensing needs that contributor's permission. The CLA
  must therefore precede the first outside contribution, not follow it
  (see §5).

### 1.5 Dependencies do not constrain the choice

- The Rust dependency ecosystem around C35 (wgpu and kin) is
  MIT/Apache-2.0 dual-licensed (wgpu itself was relicensed from MPL-2.0
  to MIT/Apache-2.0). MIT and Apache-2.0 code is compatible with every
  option considered here (Apache-2.0 with GPLv3 and AGPLv3, not with
  GPLv2 — see §2). No dependency forces or forbids any candidate license.
- Supporting evidence for the console analysis in §3: wgpu's relicense
  was motivated in part by allowing backends targeting APIs behind NDAs
  — the same NDA friction that blocks copyleft code on consoles.

## 2. Compatibility matrix (direction matters — read the arrows)

"X → Y" means "code under X may be combined into a project under Y".
Compatibility is directional: permissive code flows into copyleft
projects, never the reverse.

| Incoming code | Into AGPLv3 | Into GPLv3 | Into MPL-2.0 | Into Apache-2.0 / MIT |
|---|---|---|---|---|
| MIT / BSD / ISC | YES | YES | YES | YES |
| Apache-2.0 | YES | YES | YES | YES |
| MPL-2.0 (not marked "Incompatible With Secondary Licenses") | YES (via MPL §3.3 secondary-license clause) | YES (same clause) | YES | YES |
| MPL-2.0 marked "Incompatible With Secondary Licenses" | NO | NO | YES | YES |
| GPLv2-only | NO | NO | NO | NO |
| GPLv2-or-later (DarkPlaces) | YES — elect GPLv3, combine via AGPLv3 §13; DarkPlaces-derived files stay GPLv3 | YES — elect GPLv3, whole combination GPLv3 | NO as MPL — the combination would have to be distributed under GPL, defeating the MPL choice | NO — same reason; the combined product would be GPL-covered |
| GPLv3 | YES — AGPLv3 §13 expressly permits it | YES | NO | NO |
| AGPLv3 | YES | NO (GPLv3 has no §13-style clause admitting AGPL code; the combination must be AGPL) | NO | NO |

Two common errors this matrix kills:

1. "MPL-2.0 is GPL-compatible, so GPL code can go into an MPL project."
   Backwards. MPL-2.0's compatibility runs MPL→GPL: MPL code may be
   distributed under GPL terms. GPL code put into an MPL project must
   stay GPL, and the distributed product containing it is GPL-covered.
   An MPL engine containing DarkPlaces code is an MPL engine in name
   only for distribution purposes.
2. "GPLv2 and GPLv3 are compatible." They are not, unless the code
   carries the "or later" clause. DarkPlaces carries it; hypothetical
   GPLv2-only code does not.

## 3. The four use cases, decided per license

### (a) Adapting DarkPlaces code into C35

- AGPLv3: YES, with per-file discipline (DarkPlaces-derived files keep
  their GPLv3 notices; the rest stays AGPLv3). This is the §13 mechanism.
- GPLv3: YES, cleanest (single license across the combination).
- MPL-2.0 / Apache-2.0 / MIT: NO for any distributed combination. The
  engine would be GPL-covered the moment DarkPlaces-derived code ships
  in it. Studying DarkPlaces and reimplementing its ideas from scratch
  remains available under any license — only verbatim adaptation and
  derived code are blocked.

### (b) A modder's WASM plugin (mod-engine spec v3) calling engine APIs

First, the distinction that saves most of the mod story: **v1/v2 mods
are data, not code.** JSON prop recipes and tuning tables fed to the
engine are not derivative works of the engine and are unaffected by the
engine's license under any option. Only v3 code plugins are at issue.

- AGPLv3 / GPLv3 engine: a plugin that loads in-process and calls the
  engine's plugin ABI is, under the FSF's interpretation, part of a
  single combined work. A closed plugin is therefore NOT permitted; the
  plugin must be AGPL/GPL-compatible. (Counter-reading, noted honestly:
  a plugin communicating with the engine only over a generic protocol
  from a separate process would be a separate work — but that is not
  the spec's v3 design, which is in-process via the engine ABI, so the
  conservative reading governs planning.)
- MPL-2.0 engine: closed plugins ARE permitted. The plugin's files are
  not MPL files and do not modify MPL files, so they form a "Larger
  Work" distributable under terms of the modder's choice.
- Apache-2.0 / MIT engine: closed plugins permitted, no conditions
  beyond notice retention.

### (c) A closed commercial game built on C35

- AGPLv3 / GPLv3 engine: NOT permitted. A game linking the engine is a
  combined work that must be copyleft throughout.
- MPL-2.0 engine: permitted. Game code lives in separate files; only the
  MPL files themselves must stay MPL and their source offered.
- Apache-2.0 / MIT engine: permitted, no source duties at all.

### (d) A console port

- AGPLv3 / GPLv3: practically blocked, by two independent mechanisms.
  (1) GPLv3 §6 / AGPLv3 §6 require Installation Information for User
  Products — the ability to install and run modified versions — which
  locked consoles do not permit. (2) Console SDKs are under NDA and
  platform holders do not accept copyleft-covered submissions. This is a
  platform-policy barrier, not a line in the license text banning
  consoles — but it bites the same way.
- MPL-2.0: feasible. Only MPL files' source must be offered; the closed
  game and the NDA'd SDK code stay closed.
- Apache-2.0 / MIT: no license barrier whatsoever.

## 4. The options, for C35 and DeBrief separately

An engine and a game are not the same licensing problem. The engine's
license decides who can build on it; the game's license decides who can
copy the game. They can differ, and here they should.

### 4.1 C35 (the engine)

- **Stay AGPLv3-only.** Protects: everything, including the one case
  GPLv3 does not cover — someone taking C35, improving it, and running
  it as a hosted web service without sharing (AGPLv3 §13's remote
  network clause). C35 has a browser host, so this protection is real,
  if narrow: it bites only on hosted modified versions, not on
  downloaded games. Costs: the strongest adoption friction of any option
  (some companies ban AGPL outright on sight); closed games, closed
  plugins, and console ports all blocked. DarkPlaces door: OPEN.
- **GPLv3.** What changes vs AGPL: exactly one thing — the network
  clause is dropped. Gains: wider adoption (GPLv3 is familiar where
  AGPL is feared), simpler compliance for anyone hosting C35, and the
  cleanest DarkPlaces combination (single license). Loses: the
  hosted-service protection. Closed games, closed plugins, and console
  ports: still blocked. DarkPlaces door: OPEN, cleanest.
- **MPL-2.0.** The file-level middle path. Gains: closed games, closed
  plugins, console ports, and broad adoption. Loses: the DarkPlaces
  door CLOSES (see §2/§3a), and the guarantee that engine improvements
  come back weakens to per-file (a fork can improve C35's architecture
  in new files and never share it). Only adopt this if closed plugins
  and console matter more than DarkPlaces reuse and strong copyleft.
- **Apache-2.0.** Gains: maximum adoption, plus a patent grant with a
  retaliation clause (better than MIT for a substantial project, and
  the reason the FSF recommends Apache over other lax licenses).
  Loses: all copyleft — anyone can fork C35 closed and never contribute
  back. DarkPlaces door: CLOSED.
- **MIT.** Same as Apache minus the patent grant and retaliation
  clause. Simplest text, weakest protection. DarkPlaces door: CLOSED.
- **Dual licensing (copyleft + commercial).** The income lever, and it
  works under either AGPLv3 or GPLv3: the same code is offered free
  under copyleft and sold under a commercial license to studios that
  need closed games or console ports. Precondition, non-negotiable:
  sole copyright ownership plus a contributor agreement (CLA or
  copyright assignment) before the first outside contribution. Without
  that, dual licensing dies the day the first outside PR merges.

### 4.2 DeBrief (the game)

- **Stay AGPLv3.** Recommended. For a solo, offline-first game, the
  AGPL-vs-GPL difference is nearly worthless in practice (the network
  clause would bite only on a hosted/cloud-gaming version of the game),
  but staying AGPL costs nothing and keeps the strongest copyleft on
  the game's own systems and content. Switching the game alone to GPLv3
  gains essentially nothing — if anything changes, change the engine.
- **GPLv3 for the game.** Viable, pointless. Only reason to do it would
  be license uniformity with a GPLv3 engine, which is cosmetic: the
  AGPLv3-game + GPLv3-engine combination is expressly permitted by
  AGPLv3 §13, each part under its own license.
- **MPL-2.0 / Apache-2.0 / MIT for the game.** Only makes sense if
  Chase wants closed commercial forks of DeBrief itself — not the
  evident goal. The game's value is its canon (story, world, identity),
  which is protected by copyright in the works themselves, not by the
  code license. Keep the game copyleft.
- **Assets — open decision, flagged, not decided here.** The
  concept-art images currently sit under the repo's AGPL LICENSE with
  no separate statement. AGPL is an awkward license for art. Options:
  (1) leave art under AGPL deliberately (simplest, share-alike for art
  too); (2) carve art out under CC BY-SA 4.0 (share-alike spirit, made
  for creative works); (3) CC BY 4.0 (attribution only). Decide and
  record it in a LICENSE-ASSETS note; do not let it stay ambiguous.

### 4.3 What the recommendation is NOT

- It is not "switch to GPLv3 to unlock DarkPlaces" — that premise was
  tested and is false (§1.3).
- It is not "MPL keeps everything" — MPL closes the DarkPlaces door.
- It is not "permissive licenses keep DarkPlaces as an option" — they
  do not, for any distributed combination (§2).

## 5. Recommendation

**C35 → GPLv3 (GPL-3.0-only, matching the current "only" posture), with
dual commercial licensing prepared. DeBrief → stay AGPLv3. MPL-2.0 is
the standing alternative, adopted only if closed mod plugins and console
ports become more important than strong copyleft — and adopting it later
requires ripping out any DarkPlaces-derived code first, or keeping it
as a separate GPL component, which defeats the purpose.**

Reasoning, in order:

1. The GPLv3 switch costs exactly one protection (the hosted-service
   clause) and buys the widest adoption available without giving up
   copyleft, the cleanest DarkPlaces combination, and the dual-licensing
   income lever.
2. Staying AGPLv3 is fully viable — DarkPlaces reuse works today under
   §13 — so this is a judgment call about adoption vs the hosted-service
   protection, not a technical necessity. If Chase values the
   hosted-service protection above adoption, stay AGPLv3 and nothing
   else in this document changes.
3. DeBrief stays AGPLv3 because the game gains nothing from any switch,
   and its combination with a GPLv3 engine is expressly permitted.
4. MPL-2.0 is not recommended now because it trades away the
   DarkPlaces door and strong copyleft for benefits (closed plugins,
   console) that are not yet priorities — but it remains adoptable later
   while sole authorship holds.

### Steps to change licenses, if adopted

1. **Audit first.** Full-history contributor audit of both repos (all
   commits, all PR authors). Confirm no outside copyright holder exists
   before touching any license file. (Sample: clean. Sample is not
   proof.)
2. **C35 only.** Replace `LICENSE` with the GPLv3 text. Update the
   README license line ("GPL-3.0 — see LICENSE"). Update every
   `Cargo.toml` license field (`AGPL-3.0-only` → `GPL-3.0-only`;
   check the engine crate and every host crate). Add a copyright line
   naming the holder (currently absent — see §1.1) and per-file SPDX
   identifiers, or a central REUSE-style notice, so the declaration no
   longer rests on the README alone.
3. **DarkPlaces-derived files.** Keep their original GPL notices intact
   and mark them as GPLv3 (elected under the "or later" clause) with a
   note of their origin. Do not relicense them to anything else.
4. **DeBrief.** No license change. Optionally add a LICENSE-ASSETS note
   settling the concept-art license (§4.2).
5. **Before the first outside contribution.** Publish CONTRIBUTING with
   a contributor agreement (CLA or assignment) and require it on every
   PR. This is what keeps dual licensing alive.
6. **Dual commercial licensing (when ready).** Draft the commercial
   license offer; it is available only while sole ownership (plus CLA
   coverage) holds. Note honestly: copies already distributed under
   AGPL cannot be revoked — the relicense applies going forward.
7. **Announce the change** in both repos' READMEs with the date and the
   reason (adoption + simplicity + DarkPlaces combination), so nobody
   has to reverse-engineer it.

### Facts that would overturn this recommendation

- A full-history audit finding an outside copyright holder (relicensing
  would then need that holder's consent, and the plan changes).
- A decision that closed console ports or closed commercial games built
  on C35 are the priority → MPL-2.0 or Apache-2.0 instead, with
  DarkPlaces reuse re-scoped to study-only.
- A decision that the hosted-service protection is the most valuable
  property C35 has (e.g., C35 offered as a service) → stay AGPLv3.
- A decision that closed mod plugins matter more than copyleft →
  MPL-2.0, with the DarkPlaces cost accepted explicitly.
- Any of these is a one-line ruling from Chase; the rest of this
  document then re-reads accordingly.
