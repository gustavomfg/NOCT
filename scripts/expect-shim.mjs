// The few matchers of the release-workflow checks, on node:assert. The checks keep their assertions and messages;
// only the test runner changed from vitest to node:test. Each matcher fails with the message the caller gave.
import assert from 'node:assert/strict'

function fail(message, fallback) {
  assert.fail(message ? `${message}\n${fallback}` : fallback)
}

function matchers(received, message, negated) {
  const check = (condition, fallback) => {
    if (negated ? condition : !condition) fail(message, fallback)
  }
  const describe = (value) => JSON.stringify(value)
  return {
    toBe(expected) {
      check(Object.is(received, expected), `expected ${describe(received)} to be ${describe(expected)}`)
    },
    toEqual(expected) {
      const equal = (() => {
        try {
          assert.deepStrictEqual(received, expected)
          return true
        } catch {
          return false
        }
      })()
      check(equal, `expected ${describe(received)} to equal ${describe(expected)}`)
    },
    toMatchObject(expected) {
      const subset = Object.entries(expected).every(([key, value]) => {
        try {
          assert.deepStrictEqual(received?.[key], value)
          return true
        } catch {
          return false
        }
      })
      check(subset, `expected ${describe(received)} to match ${describe(expected)}`)
    },
    toContain(expected) {
      check(String(received).includes(expected), `expected the text to contain ${describe(expected)}`)
    },
    toMatch(pattern) {
      check(pattern instanceof RegExp ? pattern.test(received) : String(received).includes(pattern), `expected the text to match ${pattern}`)
    },
    toBeGreaterThan(value) {
      check(received > value, `expected ${received} to be greater than ${value}`)
    },
    toBeLessThan(value) {
      check(received < value, `expected ${received} to be less than ${value}`)
    },
    toHaveLength(length) {
      check(received?.length === length, `expected length ${received?.length} to be ${length}`)
    },
    toThrow(expected) {
      let thrown = null
      try {
        received()
      } catch (error) {
        thrown = error
      }
      const text = thrown ? String(thrown.message ?? thrown) : ''
      const matches =
        thrown !== null &&
        (expected === undefined ||
          (expected instanceof RegExp ? expected.test(text) : text.includes(String(expected))))
      check(matches, `expected the call to throw ${expected ?? 'an error'}, got ${thrown ? describe(text) : 'nothing'}`)
    },
  }
}

export function expect(received, message) {
  const positive = matchers(received, message, false)
  const negative = matchers(received, message, true)
  return {
    ...positive,
    not: {
      toContain: negative.toContain,
      toMatch: negative.toMatch,
      toBe: negative.toBe,
      toEqual: negative.toEqual,
    },
  }
}
