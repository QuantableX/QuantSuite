// QuantPilot parity 05: drafts for waiting requests and MCP form answers.
// Run: node --test scripts/test-pilot-requests.mjs
import assert from 'node:assert/strict'
import { test } from 'node:test'

const { requestDraft, pruneRequestDrafts, fieldError, formContent, initialValue } = await import('../modules/pilot/app/utils/requestDrafts.ts')

const fields = [
  { key: 'name', title: 'Name', required: true, kind: 'text', minLength: 2, maxLength: 5 },
  { key: 'count', title: 'Count', required: true, kind: 'integer', minimum: 1, maximum: 3, default: 2 },
  { key: 'ratio', title: 'Ratio', required: false, kind: 'number' },
  { key: 'mail', title: 'Mail', required: false, kind: 'text', format: 'email' },
  { key: 'zone', title: 'Zone', required: true, kind: 'choice', options: [{ value: 'a', label: 'A' }, { value: 'b', label: 'B' }] },
  { key: 'tags', title: 'Tags', required: false, kind: 'multi', options: [{ value: 'x', label: 'X' }], maxItems: 1 },
  { key: 'on', title: 'On', required: false, kind: 'boolean' },
]
const get = (k) => fields.find((f) => f.key === k)

test('drafts are per thread and request and survive until pruned', () => {
  const a = requestDraft('t1', 'r1')
  a.values.name = 'Ada'
  a.other['Token?'] = 'secret'
  assert.equal(requestDraft('t1', 'r1').values.name, 'Ada', 'the same draft comes back')
  assert.equal(requestDraft('t1', 'r2').values.name, undefined, 'another request has its own')
  assert.equal(requestDraft('t2', 'r1').values.name, undefined, 'another thread has its own')
  pruneRequestDrafts('t1', ['r2'])
  assert.equal(requestDraft('t1', 'r1').values.name, undefined, 'an answered request loses its draft')
  assert.equal(requestDraft('t2', 'r1').values.name, undefined)
})

test('fields check type, limits, choices and required like the backend', () => {
  assert.equal(fieldError(get('name'), ''), 'Required')
  assert.equal(fieldError(get('name'), 'A'), 'At least 2 characters')
  assert.equal(fieldError(get('name'), 'Ada'), null)
  assert.equal(fieldError(get('count'), 2.5), 'A whole number')
  assert.equal(fieldError(get('count'), 9), 'At most 3')
  assert.equal(fieldError(get('ratio'), ''), null, 'optional blank is fine')
  assert.equal(fieldError(get('mail'), 'nope'), 'Not an email address')
  assert.equal(fieldError(get('zone'), 'c'), 'Pick one of the options')
  assert.equal(fieldError(get('tags'), ['x', 'x']), 'Pick at most 1')
  assert.equal(fieldError(get('on'), null), null)
})

test('the answer leaves optional blanks out and sends typed values', () => {
  const values = Object.fromEntries(fields.map((f) => [f.key, initialValue(f)]))
  assert.equal(values.count, 2, 'the server default starts the field')
  values.name = 'Ada'
  values.zone = 'b'
  values.ratio = 1.5
  values.on = false
  assert.deepEqual(formContent(fields, values), { name: 'Ada', count: 2, ratio: 1.5, zone: 'b', on: false })
})
