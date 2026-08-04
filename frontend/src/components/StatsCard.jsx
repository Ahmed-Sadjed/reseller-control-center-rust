export default function StatsCard({ stats }) {
  if (!stats) return null;
  const items = [
    { label: 'Products', value: stats.total_products, color: 'bg-blue-500' },
    { label: 'Categories', value: stats.total_categories, color: 'bg-green-500' },
    { label: 'My Credits', value: stats.credit_balance, color: 'bg-indigo-500' },
  ];
  return (
    <div className="grid grid-cols-1 sm:grid-cols-3 gap-4">
      {items.map((item) => (
        <div key={item.label} className="bg-white rounded-lg shadow p-4 flex items-center space-x-3">
          <div className={`${item.color} w-1 h-12 rounded-full`} />
          <div>
            <p className="text-xs text-gray-500 uppercase tracking-wide">{item.label}</p>
            <p className="text-xl font-bold text-gray-900">
              {item.value !== null && item.value !== undefined ? item.value : '...'}
            </p>
          </div>
        </div>
      ))}
    </div>
  );
}
