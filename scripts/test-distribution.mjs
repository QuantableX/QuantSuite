import assert from 'node:assert/strict'
import { test } from 'node:test'
import { declaresIndicators, validateResources } from './check-distribution.mjs'

test('private sources, cached bytecode, histories and recursive copies cannot enter setup', () => {
  for (const path of [
    'smithery/indicators/my_alpha.py', 'smithery/indicators/__pycache__/my_alpha.pyc',
    'smithery/indicators/library.json', 'smithery/indicators/versions/base/child.json',
    'tests/test_private.py', 'rotation_lab/research_lces.py', '', 'smithery/', '**/*.py',
    'smithery/indicators/collection.json', 'smithery/indicators/store.json', 'QuantScript/indicators/library.json',
    'smithery/collection/abc/catalog.json', 'smithery/collection.json', 'smithery/catalog.json',
  ]) {
    assert.throws(() => validateResources({ [`../../sidecars/python/${path}`]: `sidecars/python/${path}` }), path)
  }
  assert.throws(() => validateResources({ '../../QuantScript/': 'QuantScript/' }))
  assert.throws(() => validateResources(['../../sidecars/python/']))
})

test('a source that registers indicators is recognised; the template is not', () => {
  assert.equal(declaresIndicators(['import x', '', 'REGISTER = {', '    "alpha": Alpha,', '}', ''].join('\n')), true)
  assert.equal(declaresIndicators('REGISTER: dict = {"alpha": Alpha}\n'), true)
  assert.equal(declaresIndicators('REGISTER = dict(alpha=Alpha)\n'), true)
  assert.equal(declaresIndicators(["TEMPLATE = '''", 'REGISTER = {{"{key}": {class_name}}}', "'''", ''].join('\n')), false)
  assert.equal(declaresIndicators('# REGISTER = {"alpha": Alpha}\nx = 1\n'), false)
})
