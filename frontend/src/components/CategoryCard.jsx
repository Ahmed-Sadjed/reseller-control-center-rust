import { Link } from 'react-router-dom';

const GRADIENTS = [
  'from-indigo-500 to-purple-600',
  'from-emerald-500 to-teal-600',
  'from-orange-500 to-pink-600',
  'from-cyan-500 to-blue-600',
  'from-rose-500 to-red-600',
  'from-violet-500 to-fuchsia-600',
];

export default function CategoryCard({ category, index = 0 }) {
  const gradient = GRADIENTS[index % GRADIENTS.length];

  return (
    <Link
      to={`/products?category=${category.slug}`}
      className="block group"
    >
      <div className={`relative rounded-xl overflow-hidden shadow-lg transition-transform hover:scale-[1.02] ${category.image ? '' : `bg-gradient-to-br ${gradient}`}`}>
        {category.image && (
          <div className="absolute inset-0">
            <img
              src={category.image}
              alt={category.name}
              className="w-full h-full object-cover"
            />
            <div className="absolute inset-0 bg-gradient-to-t from-black/70 via-black/30 to-transparent" />
          </div>
        )}
        <div className={`relative p-6 ${category.image ? 'text-white min-h-[180px] flex flex-col justify-end' : 'text-white'}`}>
          <h3 className="text-xl font-bold">{category.name}</h3>
          {category.description && (
            <p className="text-sm text-white/80 mt-1 line-clamp-2">{category.description}</p>
          )}
          <div className="mt-4 flex items-center justify-between">
            <span className="text-sm font-medium text-white/90">
              {category.product_count} product{category.product_count !== 1 ? 's' : ''}
            </span>
            <span className="text-white/60 group-hover:translate-x-1 transition-transform">&rarr;</span>
          </div>
        </div>
      </div>
    </Link>
  );
}
