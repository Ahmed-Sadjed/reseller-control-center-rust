export default function CategoryCardSkeleton() {
  return (
    <div className="block">
      <div className="rounded-xl overflow-hidden shadow-lg bg-gradient-to-br from-gray-200 to-gray-300 animate-pulse">
        <div className="relative p-6 min-h-[180px] flex flex-col justify-end">
          <div className="h-6 w-32 bg-white/30 animate-pulse rounded mb-2" />
          <div className="h-4 w-full bg-white/20 animate-pulse rounded mb-1" />
          <div className="h-4 w-2/3 bg-white/20 animate-pulse rounded mb-4" />
          <div className="flex items-center justify-between">
            <div className="h-4 w-28 bg-white/30 animate-pulse rounded" />
            <div className="h-4 w-4 bg-white/30 animate-pulse rounded" />
          </div>
        </div>
      </div>
    </div>
  );
}