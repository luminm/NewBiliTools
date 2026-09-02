import { SourceMapConsumer, RawSourceMap, MappedPosition } from 'source-map-js';
import StackTrace from 'stacktrace-js';
import { TauriError } from './backend';
import { AppLog, type AppLogOptions } from './utils';

const cache = new Map<string, SourceMapConsumer>();
if (import.meta.env.PROD) getConsumer(import.meta.url);

async function getConsumer(url: string) {
  if (cache.has(url)) {
    return cache.get(url)!;
  }
  try {
    const resp = await window.fetch(url + '.map');
    if (!resp.ok) throw null;
    const map = (await resp.json()) as RawSourceMap;
    const consumer = new SourceMapConsumer(map);
    cache.set(url, consumer);
    return consumer;
  } catch {
    return null;
  }
}

const TRANSIENT_CODES = new Set([
  408, 425, 429, 500, 502, 503, 504, 509, 520, 521, 522, 523, 524, 525, 527,
]);

export class AppError extends Error {
  original?: Error;
  code?: number | string;
  transient?: boolean;
  constructor(
    input: unknown,
    options?: { code?: number | string; name?: string; transient?: boolean },
  ) {
    const data = AppError.parse(input, options);
    super(data.message);
    this.name = data.name;
    this.original = data.original;
    this.code = data.code;
    this.transient = data.transient;
  }
  static parse(
    input: unknown,
    options?: { code?: number | string; name?: string; transient?: boolean },
  ): {
    message: string;
    name: string;
    original?: Error;
    code?: number | string;
    transient?: boolean;
  } {
    if (input instanceof AppError) {
      return {
        message: input.message,
        name: input.name,
        code: input.code,
        transient: input.transient,
        original: input.original ?? input,
      };
    }
    if (input instanceof Error) {
      return {
        message: input.message,
        name: options?.name ?? input.name ?? 'AppError',
        code: options?.code,
        transient: options?.transient ?? AppError.isTransient(input.message, options?.code),
        original: input,
      };
    }
    if (typeof input === 'string') {
      return {
        message: options?.code ? `${input} (${options.code})` : input,
        name: options?.name ?? 'AppError',
        code: options?.code,
        transient: options?.transient ?? AppError.isTransient(input, options?.code),
      };
    }
    // fallback
    const err = input as TauriError;
    let message = err.message;
    if (err.code !== null) {
      message += ` (${err.code})`;
    }
    if (err.stack) {
      message += `\n${err.stack}`;
    }
    return {
      message,
      name: options?.name ?? 'AppError',
      code: err.code ?? options?.code,
      transient:
        options?.transient ?? AppError.isTransient(err.message, err.code ?? undefined),
    };
  }
  static isTransient(message: string, code?: number | string) {
    if (typeof code === 'number' && TRANSIENT_CODES.has(code)) {
      return true;
    }
    const text = String(message ?? '').toLowerCase();
    return /(timeout|timed out|temporarily|temporary|rate limit|too many requests|server error|service unavailable|bad gateway|network error|connection (reset|refused)|unavailable|retry)/.test(
      text,
    );
  }
  isTransient() {
    return this.transient ?? AppError.isTransient(this.message, this.code);
  }
  async handle(options?: AppLogOptions) {
    const frames = await StackTrace.fromError(this.original ?? this);
    if (import.meta.env.DEV) {
      console.log('Got StackFrames for ' + this.message + '\n', frames);
      const stack = (this.original ?? this).stack;
      const type = this.isTransient() ? 'warning' : 'error';
      return AppLog(stack ?? '', type, {
        ...options,
        toast: options?.toast ?? !this.isTransient(),
      });
    }
    const stack: string[] = [];
    const raw: MappedPosition[] = [];
    for (const v of frames.filter((v) => v.fileName)) {
      const f = v.fileName!;
      const l = v.lineNumber;
      const c = v.columnNumber;
      const consumer = await getConsumer(new URL(f, import.meta.url).href);
      if (consumer && l && c) {
        const orig = consumer.originalPositionFor({
          line: l,
          column: c,
          bias: SourceMapConsumer.GREATEST_LOWER_BOUND,
        });
        if (orig.source.startsWith('/node_modules')) {
          continue;
        }
        let line = `${orig.source}:${orig.line}:${orig.column}`;
        if (orig.name) line += ` (${orig.name})`;
        stack.push('    at ' + line);
        raw.push(orig);
      } else {
        stack.push('<anonymous>');
      }
    }
    console.log('Got MappedPositions for ' + this.message + '\n', raw);
    const type = this.isTransient() ? 'warning' : 'error';
    return AppLog(
      `${this.name}: ${this.message}\n` + stack.join('\n'),
      type,
      {
        ...options,
        toast: options?.toast ?? !this.isTransient(),
      },
    );
  }
}
