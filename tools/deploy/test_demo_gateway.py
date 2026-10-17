import concurrent.futures
import tempfile
import unittest
from pathlib import Path
from demo_gateway import Ledger

class BudgetTests(unittest.TestCase):
    def test_global_admission_is_atomic_persistent_and_idempotent(self):
        with tempfile.TemporaryDirectory() as folder:
            path = str(Path(folder) / 'ledger.db')
            ledger = Ledger(path)
            with concurrent.futures.ThreadPoolExecutor(max_workers=12) as pool:
                results = list(pool.map(lambda i: ledger.reserve('visitor'+str(i), str(i)), range(30)))
            self.assertEqual(sum(x is not None for x in results), 4)
            winner = next(i for i,x in enumerate(results) if x is not None)
            self.assertEqual(Ledger(path).reserve('visitor'+str(winner), str(winner)), results[winner])
            self.assertIsNone(Ledger(path).reserve('new', 'new'))
            ledger.attach(results[winner], 'run')
            self.assertTrue(ledger.admitted('visitor'+str(winner), 'run'))
            self.assertFalse(ledger.admitted('someone-else', 'run'))

if __name__ == '__main__': unittest.main()
