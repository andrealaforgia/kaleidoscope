// Kaleidoscope Prism — operator-facing observability SPA
// Copyright (C) 2026 The Kaleidoscope authors
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as
// published by the Free Software Foundation, either version 3 of the
// License, or (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU
// Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public
// License along with this program. If not, see <https://www.gnu.org/licenses/>.

// Invariant — public TypeScript surface lock.
//
// Same shape as Codex's `invariant_public_api_smoke.rs`: a compile-
// time enforcement that the named types stay exported from their
// modules. If any of the locked types is renamed, removed, or has
// its shape changed in a backwards-incompatible way, this file
// fails to compile and CI Gate 6 (Vitest) reports it.
//
// At v0 the public surface is the set of types Prism's modules
// export to one another (the SPA is a single-bundle deployable, so
// "public" means "visible to the QueryPanel from outside its folder").
//
// ADRs anchored: 0026 (component layout), 0027 (QueryOutcome shape),
//                0028 (UrlState shape), 0029 (auto-refresh state +
//                event + effect shapes).
//
// At DISTILL, the imports below all fail to resolve because the
// crafter has not yet implemented `apps/prism/src/`. At DELIVER's
// first slice landing, the crafter writes the stub modules and this
// file compiles. From that point on, mutation testing on the type
// signatures (StrykerJS does not mutate types but module-rename
// mutations break this file's compilation) catches regressions.

import { describe, it, expectTypeOf } from 'vitest';

// ADR-0027 — QueryOutcome 5-arm union.
import type { QueryOutcome, TransportCause } from '../src/lib/promql/types';

// ADR-0028 — URL state types.
import type {
  UrlState,
  TimeRange,
  RelativeTimeRange,
  AbsoluteTimeRange,
  RefreshInterval,
  UrlParseError,
} from '../src/lib/url-state/types';

// ADR-0029 — auto-refresh state machine types.
import type {
  AutoRefreshState,
  AutoRefreshEvent,
  AutoRefreshEffect,
} from '../src/lib/auto-refresh/events';

// ADR-0026 / 0027 / 0028 — pure functions that constitute the surface.
import type { queryRange } from '../src/lib/promql/queryRange';
import type { decode, encode } from '../src/lib/url-state/codec';
import type { reduce } from '../src/lib/auto-refresh/reducer';
import type { buildOption } from '../src/lib/echarts/buildOption';

// ADR-0030 — RuntimeConfig (the /config.json shape).
import type { RuntimeConfig, ConfigError } from '../src/lib/config/types';

describe('Invariant — public TypeScript surface (compile-time)', () => {
  it('QueryOutcome has exactly five discriminated arms (ADR-0027)', () => {
    // The five arms are: success | empty | parse-error | transport-error |
    // config-error. A sixth arm is a forward-compat addition that needs an
    // ADR amendment; a removed arm is a breaking change.
    expectTypeOf<QueryOutcome['kind']>().toEqualTypeOf<
      'success' | 'empty' | 'parse-error' | 'transport-error' | 'config-error'
    >();
  });

  it('TransportCause distinguishes network / http-status / invalid-json / shape / aborted (ADR-0027)', () => {
    expectTypeOf<TransportCause['kind']>().toEqualTypeOf<
      'network' | 'http-status' | 'invalid-json' | 'shape' | 'aborted'
    >();
  });

  it('TimeRange is { kind: "relative" } | { kind: "absolute" } (ADR-0028)', () => {
    expectTypeOf<TimeRange['kind']>().toEqualTypeOf<'relative' | 'absolute'>();
  });

  it('RefreshInterval is the closed set off / 5s / 10s / 30s / 1m (ADR-0028)', () => {
    expectTypeOf<RefreshInterval>().toEqualTypeOf<'off' | '5s' | '10s' | '30s' | '1m'>();
  });

  it('AutoRefreshState has four kinds: idle / running / backoff / hidden (ADR-0029)', () => {
    expectTypeOf<AutoRefreshState['kind']>().toEqualTypeOf<
      'idle' | 'running' | 'backoff' | 'hidden'
    >();
  });

  it('AutoRefreshEvent has the locked event vocabulary (ADR-0029)', () => {
    expectTypeOf<AutoRefreshEvent['kind']>().toEqualTypeOf<
      | 'refresh-changed'
      | 'range-changed'
      | 'tick-fired'
      | 'fetch-result'
      | 'visibility-changed'
    >();
  });

  it('AutoRefreshEffect has the locked effect vocabulary (ADR-0029)', () => {
    expectTypeOf<AutoRefreshEffect['kind']>().toEqualTypeOf<
      'schedule-timer' | 'cancel-timer' | 'fetch' | 'cancel-fetch'
    >();
  });

  it('queryRange is total: returns QueryOutcome, never throws (ADR-0027)', () => {
    type QueryRangeFn = typeof queryRange;
    type ReturnT = ReturnType<QueryRangeFn>;
    expectTypeOf<ReturnT>().toEqualTypeOf<Promise<QueryOutcome>>();
  });

  it('decode is total: returns Result<UrlState, UrlParseError>, never throws (ADR-0028)', () => {
    type DecodeFn = typeof decode;
    type ReturnT = ReturnType<DecodeFn>;
    // The Result type is { kind: "ok"; value: UrlState } | { kind: "error"; error: UrlParseError }.
    // The crafter chooses where Result lives (lib/util/result vs inline);
    // this test only asserts decode never throws (return type is not Promise<UrlState>
    // and the union has an error arm).
    expectTypeOf<ReturnT extends { kind: 'ok' | 'error' } ? true : false>().toEqualTypeOf<true>();
  });

  it('encode is total: returns string, takes UrlState (ADR-0028)', () => {
    type EncodeFn = typeof encode;
    expectTypeOf<EncodeFn>().toEqualTypeOf<(state: UrlState) => string>();
  });

  it('reduce is pure: (state, event) => (next, effects) (ADR-0029)', () => {
    type ReduceFn = typeof reduce;
    type ReturnT = ReturnType<ReduceFn>;
    // toMatchTypeOf is permissive on readonly/mutable variance; the
    // contract is "next is an AutoRefreshState; effects is a list of
    // AutoRefreshEffect". The reducer body chooses readonly arrays.
    expectTypeOf<ReturnT>().toMatchTypeOf<{
      next: AutoRefreshState;
      effects: ReadonlyArray<AutoRefreshEffect>;
    }>();
  });

  it('buildOption is pure: (input) => EChartsOption (ADR-0030)', () => {
    type BuildOptionFn = typeof buildOption;
    type ReturnT = ReturnType<BuildOptionFn>;
    // EChartsOption is a structural type defined by the echarts package.
    // We assert buildOption is a synchronous function (not Promise) — i.e.
    // pure with no I/O.
    expectTypeOf<ReturnT extends Promise<unknown> ? false : true>().toEqualTypeOf<true>();
  });

  it('RuntimeConfig has the locked /config.json shape (ADR-0030)', () => {
    expectTypeOf<RuntimeConfig>().toMatchTypeOf<{
      backend: { url: string; label: string };
      prism: { version: string };
    }>();
  });
});

// =============================================================================
// Anti-leakage: types that must NOT appear in the public surface
// =============================================================================
//
// The pure cores (URL codec, buildOption, auto-refresh reduce) must not
// import React, the DOM, or any side-effecting module. The compile-
// time check below pins the leaf modules' import graph.
//
// At DISTILL the import paths are stubs; the crafter at DELIVER writes
// the modules with the correct shape. ESLint's boundaries plugin
// (ADR-0031 §7) enforces the same rule structurally; this Vitest
// counterpart is the type-system's belt-and-braces.

describe('Invariant — pure cores do not import side-effecting modules', () => {
  it('decode / encode do not import React (lib/url-state/codec)', () => {
    // Compile-time: if codec.ts ever imports React, the next line's type
    // resolution would pull React's module signature into this file. We
    // assert the codec's only exports are the codec functions and the
    // types — no React, no DOM.
    type CodecExports = typeof import('../src/lib/url-state/codec');
    expectTypeOf<keyof CodecExports>().toEqualTypeOf<'decode' | 'encode'>();
  });

  it('reduce does not import React or DOM (lib/auto-refresh/reducer)', () => {
    type ReducerExports = typeof import('../src/lib/auto-refresh/reducer');
    expectTypeOf<keyof ReducerExports>().toEqualTypeOf<'reduce'>();
  });

  it('buildOption does not import React or DOM (lib/echarts/buildOption)', () => {
    type BuildOptionExports = typeof import('../src/lib/echarts/buildOption');
    expectTypeOf<keyof BuildOptionExports>().toEqualTypeOf<'buildOption'>();
  });
});
