export default function ProductCardSkeleton() {
  return (
    <div className="bg-white rounded-lg shadow-md border border-gray-200 overflow-hidden">
      <div className="aspect-video bg-gray-200 animate-pulse" />
      <div className="p-6 space-y-3">
        <div className="flex items-start justify-between gap-2">
          <div className="h-5 w-36 bg-gray-200 animate-pulse rounded" />
          <div className="h-5 w-16 bg-gray-200 animate-pulse rounded" />
        </div>
        <div className="h-4 w-full bg-gray-200 animate-pulse rounded" />
        <div className="flex items-center justify-between pt-2">
          <div className="h-7 w-20 bg-gray-200 animate-pulse rounded" />
          <div className="h-4 w-12 bg-gray-200 animate-pulse rounded" />
        </div>
        <div className="flex flex-wrap gap-1.5 pt-2">
          <div className="h-8 w-20 bg-gray-200 animate-pulse rounded" />
          <div className="h-8 w-24 bg-gray-200 animate-pulse rounded" />
        </div>
        <div className="flex items-center space-x-3 pt-2">
          <div className="h-4 w-8 bg-gray-200 animate-pulse rounded" />
          <div className="h-9 w-16 bg-gray-200 animate-pulse rounded" />
          <div className="flex-1 h-9 bg-gray-200 animate-pulse rounded" />
        </div>
      </div>
    </div>
  );
}