export default function BalanceHeader({ balance }) {
  return (
    <div className="bg-indigo-600 text-white px-6 py-3 rounded-lg shadow-md">
      <div className="flex items-center justify-between">
        <span className="text-lg font-semibold">Credit Balance</span>
        <span className="text-2xl font-bold">{balance !== null ? balance : '...'}</span>
      </div>
    </div>
  );
}
