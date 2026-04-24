import { useState, useEffect } from "react";

interface CountdownResult {
  hours: number;
  minutes: number;
  seconds: number;
  percentage: number;
  isOverdue: boolean;
}

export const useHarvestCountdown = (nextRun: Date): CountdownResult => {
  const [countdown, setCountdown] = useState<CountdownResult>({
    hours: 0,
    minutes: 0,
    seconds: 0,
    percentage: 0,
    isOverdue: false,
  });

  useEffect(() => {
    const calculateCountdown = () => {
      const now = Date.now();
      const target = nextRun.getTime();
      const diff = target - now;

      if (diff <= 0) {
        setCountdown({
          hours: 0,
          minutes: 0,
          seconds: 0,
          percentage: 100,
          isOverdue: true,
        });
        return;
      }

      const totalSeconds = Math.floor(diff / 1000);
      const hours = Math.floor(totalSeconds / 3600);
      const minutes = Math.floor((totalSeconds % 3600) / 60);
      const seconds = totalSeconds % 60;

      const TWENTY_FOUR_HOURS = 24 * 60 * 60;
      const elapsed = TWENTY_FOUR_HOURS - totalSeconds;
      const percentage = Math.min(
        100,
        Math.max(0, (elapsed / TWENTY_FOUR_HOURS) * 100),
      );

      setCountdown({
        hours,
        minutes,
        seconds,
        percentage,
        isOverdue: false,
      });
    };

    calculateCountdown();
    const interval = setInterval(calculateCountdown, 1000);

    return () => clearInterval(interval);
  }, [nextRun]);

  return countdown;
};
