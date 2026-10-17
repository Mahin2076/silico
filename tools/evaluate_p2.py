#!/usr/bin/env python3
"""Bounded P2 quality evaluation on labeled synthetic cases, never survey validity.

Uses the actual p2_verify binary. --live is required to spend model credits.
Set CRUSOE_API_KEY in the process environment; credentials are never loaded/logged.
At most eight cases and two concurrent calls; no retries or model fallback.
"""
import argparse
import concurrent.futures
import copy
import datetime
import hashlib
import json
import pathlib
import statistics
import subprocess
import time


def cases(root):
    base = json.loads((root / 'crates/sim-core/src/verifier/fixtures/complete.json').read_text())
    specs = [
        ('valid_comparison', True, None),
        ('valid_reverse_comparison', True, 'In this labeled synthetic software fixture, message A has modeled support of 60%, lower than message B at 65%, a five-percentage-point difference. These estimates are not observed public opinion or evidence of real-world predictive validity.'),
        ('wrong_direction', False, 'In this labeled synthetic software fixture, message A has higher modeled support than message B (60% versus 65%). These are not observed public opinion or evidence of predictive validity.'),
        ('wrong_magnitude', False, 'In this labeled synthetic software fixture, message B has 65% modeled support versus 60% for A, a fifty-percentage-point advantage. These are not observed public opinion or evidence of predictive validity.'),
        ('invented_number', False, 'In this labeled synthetic software fixture, message B has 85% modeled support versus 60% for A, a twenty-five-percentage-point advantage. These are not observed public opinion or evidence of predictive validity.'),
        ('causal_guarantee', False, 'This proves message B causes every real San Francisco resident to support the policy, with guaranteed predictive accuracy.'),
        ('unsupported_external_fact', False, 'An official real-world survey proves that 75% of San Francisco residents prefer message B. The synthetic fixture has 65% versus 60% and is not itself an observed survey.'),
        ('irrelevant_citation', False, None),
    ]
    for name, valid, conclusion in specs:
        request = copy.deepcopy(base)
        request['run_id'] = 'quality-' + name.replace('_', '-')
        if conclusion is not None:
            request['conclusions'] = [conclusion]
        if name == 'irrelevant_citation':
            # Explicit synthetic source: tests reviewer relevance judgment. This
            # is not a You.com live search and cannot grant live completion.
            request['web_research'] = {
                'provider': 'you.com', 'mode': 'mock',
                'query': {'query': 'synthetic survey limitations', 'include_domains': []},
                'as_of_date': request['plan']['context']['as_of_date'],
                'fetched_at': request['plan']['context']['as_of_date'] + 'T00:00:00Z',
                'status': 'ready', 'search_calls': 1, 'error_code': None,
                'sources': [{'id': 'web-1', 'title': 'Bread baking instructions — explicit irrelevant fixture',
                    'url': 'https://example.org/bread', 'snippets': ['Combine flour, yeast and water. Let bread dough rise before baking.'], 'publication_date': None}],
            }
        yield name, valid, request


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', required=True, type=pathlib.Path)
    parser.add_argument('--output-dir', required=True, type=pathlib.Path)
    parser.add_argument('--live', action='store_true', help='Spend at most eight bounded Nemotron requests.')
    args = parser.parse_args()
    if not args.live:
        parser.error('--live is required; no implicit paid calls or mock substitute')
    root = pathlib.Path(__file__).resolve().parent.parent
    binary = args.binary.resolve(strict=True)
    out = args.output_dir.resolve()
    out.mkdir(parents=True, exist_ok=True)
    started = datetime.datetime.now(datetime.timezone.utc).isoformat()

    def evaluate(item):
        name, expected, request = item
        input_path, output_path = out / (name + '-input.json'), out / (name + '-review.json')
        input_path.write_text(json.dumps(request, indent=2) + '\n')
        begin = time.monotonic()
        process = subprocess.run([str(binary), '--input', str(input_path), '--live', '--output', str(output_path)],
                                 capture_output=True, text=True, timeout=45)
        elapsed = round((time.monotonic() - begin) * 1000)
        decision = json.loads(output_path.read_text()) if output_path.exists() else None
        error = decision['receipt']['error_code'] if decision else 'invocation_failed'
        passed = decision is not None and decision['verdict'] == 'pass' and error is None
        return {'case': name, 'expected_accept': expected, 'accepted_verdict': passed,
                'correct_verdict': passed == expected, 'validated_semantic_review': decision is not None and error is None,
                'error_code': error, 'exit_code': process.returncode, 'elapsed_ms': elapsed,
                'model_calls': decision['receipt']['model_calls'] if decision else 0,
                'synthetic_web_fixture': name == 'irrelevant_citation',
                'input_sha256': hashlib.sha256(input_path.read_bytes()).hexdigest()}

    with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
        rows = list(pool.map(evaluate, cases(root)))
    bad, good = [r for r in rows if not r['expected_accept']], [r for r in rows if r['expected_accept']]
    report = {'checked_at': started, 'scope': 'Labeled synthetic software cases; real Nemotron review. Not population predictive validity.',
              'false_accepts': sum(r['accepted_verdict'] for r in bad), 'invalid_cases': len(bad),
              'valid_accepts': sum(r['accepted_verdict'] for r in good), 'valid_cases': len(good),
              'validated_semantic_reviews': sum(r['validated_semantic_review'] for r in rows),
              'protocol_or_provider_failures': sum(not r['validated_semantic_review'] for r in rows),
              'median_elapsed_ms': statistics.median(r['elapsed_ms'] for r in rows),
              'total_model_calls': sum(r['model_calls'] for r in rows), 'max_concurrent': 2, 'automatic_retries': 0, 'cases': rows}
    (out / 'evaluation.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report, indent=2))
    return 0 if all(r['correct_verdict'] and r['validated_semantic_review'] for r in rows) else 1


if __name__ == '__main__':
    raise SystemExit(main())
