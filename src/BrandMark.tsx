import { useId } from "react";

export function BrandMark({ className }: { className?: string }) {
  const clip = useId();
  return (
    <svg className={className} viewBox="260 390 730 410" role="img" aria-label="SkyDek logo">
      <defs>
        <clipPath id={clip}>
          <path d="M277 632 417 449Q459 399 519 399H984L920 469H548Q503 469 476 505L430 559Q411 587 445 621L440 632Z" />
          <path d="M267 789 328 725H755Q791 725 810 695L837 659Q868 622 802 577H871Q917 577 932 613Q944 639 917 673L864 744Q832 789 775 789Z" />
          <path d="M452 637 487 560Q501 525 535 515Q559 504 573 517Q580 523 597 523H673Q687 523 697 515Q711 506 740 518Q765 526 780 562L807 628Q818 653 807 673Q792 703 763 691Q753 687 737 671L713 646Q700 631 680 631H580Q561 631 546 647L514 679Q489 705 465 685Q444 669 452 637Z" />
        </clipPath>
      </defs>
      <image href="/brand.png" width="1254" height="1254" clipPath={`url(#${clip})`} />
    </svg>
  );
}
