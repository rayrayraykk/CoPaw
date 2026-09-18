"""Emit legacy scheduler slots for Rust-only test fixtures."""

import json
import sys
from datetime import datetime, timedelta, timezone
from zoneinfo import ZoneInfo

import apscheduler
from apscheduler.triggers.cron import CronTrigger
from apscheduler.triggers.date import DateTrigger
from apscheduler.triggers.interval import IntervalTrigger


def slots(case):
    """Evaluate one normalized legacy schedule without running any job."""
    spec = case[f"schedule"]
    zone = ZoneInfo(spec[f"timezone"])
    if spec[f"type"] == f"cron":
        trigger = CronTrigger.from_crontab(spec[f"cron"], timezone=zone)
    else:
        start = datetime.fromisoformat(spec[f"run_at"])
        if start.tzinfo is None:
            start = start.replace(tzinfo=zone)
        days = spec.get(f"repeat_every_days")
        end = None
        if spec.get(f"repeat_end_type") == f"count":
            end = start + timedelta(days=days * (spec[f"repeat_count"] - 1))
        elif spec.get(f"repeat_end_type") == f"until":
            end = datetime.fromisoformat(spec[f"repeat_until"])
        if days:
            trigger = IntervalTrigger(
                days=days, start_date=start, end_date=end, timezone=zone,
            )
        else:
            trigger = DateTrigger(run_date=start, timezone=zone)
    now = datetime.fromisoformat(case[f"now"]).astimezone(zone)
    previous = None
    result = []
    for _ in range(case[f"count"]):
        value = trigger.get_next_fire_time(previous, now)
        result.append(
            value.astimezone(timezone.utc).isoformat() if value else None,
        )
        if value is None:
            break
        previous = value
        now = value
    return result


if __name__ == f"__main__":
    print(json.dumps({
        f"apscheduler": apscheduler.__version__,
        f"slots": [slots(case) for case in json.load(sys.stdin)],
    }))
