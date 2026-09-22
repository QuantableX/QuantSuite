import assert from 'node:assert/strict'
import { test } from 'node:test'
import { validateResources } from './check-distribution.mjs'

test('private sources, cached bytecode, histories and recursive copies cannot enter setup', () => {
  for (const path of [
    'smithery/indicators/my_alpha.py', 'smithery/indicators/__pycache__/my_alpha.pyc',
    'smithery/indicators/library.json', 'smithery/indicators/versions/base/child.json',
    'tests/test_private.py', 'rotation_lab/research_lces.py', '', 'smithery/', '**/*.py',
  ]) {
    assert.throws(() => validateResources({ [`../../sidecars/python/${path}`]: `sidecars/python/${path}` }), path)
  }
  assert.throws(() => validateResources({ '../../QuantScript/': 'QuantScript/' }))
  assert.throws(() => validateResources(['../../sidecars/python/']))
})
