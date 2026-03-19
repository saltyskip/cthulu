#!/usr/bin/env python3
"""
Slack Message Fetcher — class-based, minimal.

Usage:
  export SLACK_USER_TOKEN=xoxp-...
  python slack_messages.py                                  # today's unread
  python slack_messages.py --yesterday --all                # all yesterday
  python slack_messages.py --hours 2                        # last 2 hours
  python slack_messages.py --days 3 --mentions-me --all     # @mentions, 3 days
  python slack_messages.py --mentions devopsinfra --all     # group mentions
  python slack_messages.py --dms-only --hours 1             # recent DMs
  python slack_messages.py --channels-only --channel general,devops --json
"""

import argparse
import json
import os
import sys
import time
from datetime import datetime, timedelta, timezone

try:
    from slack_sdk import WebClient
    from slack_sdk.errors import SlackApiError
except ImportError:
    sys.exit("pip install slack_sdk")


class SlackFetcher:
    def __init__(self, token: str):
        self.client = WebClient(token=token)
        self.auth = self._call(self.client.auth_test)
        self.my_id = self.auth["user_id"]
        self.users = self._build_user_map()

    def _call(self, fn, *args, retries=5, **kwargs):
        """Slack API call with automatic rate-limit retry."""
        for attempt in range(retries):
            try:
                return fn(*args, **kwargs)
            except SlackApiError as e:
                if e.response["error"] == "ratelimited":
                    wait = int(e.response.headers.get("Retry-After", 5)) + attempt
                    print(f"  rate-limited, retrying in {wait}s...", file=sys.stderr)
                    time.sleep(wait)
                    continue
                raise
        raise RuntimeError(f"Still rate-limited after {retries} retries")

    def _paginate(self, fn, key="channels", **kwargs):
        """Generic paginated Slack API call."""
        items, cursor = [], None
        while True:
            resp = self._call(fn, cursor=cursor, limit=200, **kwargs)
            items.extend(resp.get(key, []))
            cursor = resp.get("response_metadata", {}).get("next_cursor")
            if not cursor:
                break
            time.sleep(0.5)
        return items

    def _build_user_map(self) -> dict:
        users = {}
        for u in self._paginate(self.client.users_list, key="members"):
            p = u.get("profile", {})
            users[u["id"]] = p.get("display_name") or p.get("real_name") or u.get("name", u["id"])
        return users

    def conv_label(self, conv: dict) -> str:
        if conv.get("is_im"):
            return f"DM: {self.users.get(conv.get('user', ''), conv['id'])}"
        if conv.get("is_mpim"):
            return f"Group DM: {conv.get('name', conv['id'])}"
        prefix = "\U0001f512" if conv.get("is_private") else "#"
        return f"{prefix}{conv.get('name', conv['id'])}"

    def resolve_mentions(self, targets: list[str]) -> set:
        """Resolve user/group names to Slack mention patterns."""
        patterns = set()
        targets = [t.lower().lstrip("@") for t in targets]

        try:
            for g in self._call(self.client.usergroups_list, include_users=False).get("usergroups", []):
                if g.get("handle", "").lower() in targets or g.get("name", "").lower() in targets:
                    patterns.add(f"<!subteam^{g['id']}")
                    print(f"  Resolved @{g['handle']} -> {g['id']}", file=sys.stderr)
        except (SlackApiError, RuntimeError):
            print("  Warning: couldn't list usergroups (need usergroups:read scope)", file=sys.stderr)

        reverse = {name.lower(): uid for uid, name in self.users.items()}
        for t in targets:
            if t in reverse:
                patterns.add(f"<@{reverse[t]}")
                print(f"  Resolved @{t} -> {reverse[t]}", file=sys.stderr)

        return patterns

    def list_conversations(self, types: str, channel_filter: list[str] | None = None) -> list:
        convos = self._paginate(self.client.conversations_list, types=types, exclude_archived=True)
        convos = [c for c in convos if c.get("is_im") or c.get("is_mpim") or c.get("is_member")]
        if channel_filter:
            names = {n.lower().lstrip("#") for n in channel_filter if n.strip()}
            convos = [c for c in convos if c.get("name", "").lower() in names]
        return convos

    def fetch_thread_replies(self, channel_id: str, thread_ts: str) -> list:
        """Fetch all replies in a thread."""
        try:
            # NOTE: limit=200 without cursor-based pagination means threads with
            # 200+ replies will be silently truncated. Acceptable for dashboard use.
            resp = self._call(self.client.conversations_replies,
                              channel=channel_id, ts=thread_ts, limit=200)
            if resp.get("has_more", False):
                print(f"  Warning: thread {thread_ts} has more than 200 replies, truncated", file=sys.stderr)
            replies = resp.get("messages", [])
            # First message is the parent — skip it, return only replies
            return replies[1:] if len(replies) > 1 else []
        except (SlackApiError, RuntimeError) as e:
            print(f"  Skipping thread {thread_ts}: {e}", file=sys.stderr)
            return []

    def fetch(self, oldest: float, latest: float, conv_types: str,
              read_filter: str = "unread", mention_patterns: set | None = None,
              channel_filter: list[str] | None = None, debug: bool = False,
              with_threads: bool = False) -> list:
        """Fetch and filter messages. Returns list of {channel, count, messages}."""
        convos = self.list_conversations(conv_types, channel_filter)
        if debug:
            print(f"[debug] Found {len(convos)} conversations", file=sys.stderr)
            print(f"[debug] Time window: {datetime.fromtimestamp(oldest, tz=timezone.utc)} -> {datetime.fromtimestamp(latest, tz=timezone.utc)}", file=sys.stderr)
        results = []

        for conv in convos:
            cid, name = conv["id"], self.conv_label(conv)
            last_read = float(conv.get("last_read") or 0)

            try:
                resp = self._call(self.client.conversations_history,
                                  channel=cid, oldest=str(oldest), latest=str(latest), limit=200)
            except (SlackApiError, RuntimeError) as e:
                print(f"  Skipping {name}: {e}", file=sys.stderr)
                continue
            time.sleep(0.4)

            msgs = resp.get("messages", [])
            if resp.get("has_more", False):
                print(f"  Warning: #{name} has more than 200 messages in this window, oldest messages truncated", file=sys.stderr)
            if debug:
                print(f"\n[debug] {name}: {len(msgs)} raw messages", file=sys.stderr)
                for m in msgs[:5]:
                    print(f"  [debug] raw: {m.get('text', '')[:200]}", file=sys.stderr)

            if not msgs:
                continue

            if read_filter == "unread":
                msgs = [m for m in msgs if float(m.get("ts", 0)) > last_read and m.get("user") != self.my_id]
            elif read_filter == "read":
                msgs = [m for m in msgs if float(m.get("ts", 0)) <= last_read]

            if mention_patterns:
                if debug:
                    for m in msgs[:5]:
                        text = m.get("text", "")
                        matched = any(p in text for p in mention_patterns)
                        print(f"  [debug] mention match={matched}: {text[:150]}", file=sys.stderr)
                msgs = [m for m in msgs if any(p in m.get("text", "") for p in mention_patterns)]

            if not msgs:
                continue

            msgs.sort(key=lambda m: float(m.get("ts", 0)))
            formatted = []
            for m in msgs:
                msg_data = {
                    "time": datetime.fromtimestamp(float(m["ts"]), tz=timezone.utc).isoformat(),
                    "user": self.users.get(m.get("user", ""), "unknown"),
                    "text": m.get("text", ""),
                    "ts": m["ts"],
                }
                reply_count = m.get("reply_count", 0)
                if with_threads and reply_count > 0:
                    msg_data["thread_ts"] = m.get("thread_ts", m["ts"])
                    msg_data["reply_count"] = reply_count
                    raw_replies = self.fetch_thread_replies(cid, m["ts"])
                    msg_data["replies"] = [
                        {
                            "time": datetime.fromtimestamp(float(r["ts"]), tz=timezone.utc).isoformat(),
                            "user": self.users.get(r.get("user", ""), "unknown"),
                            "text": r.get("text", ""),
                            "ts": r["ts"],
                        }
                        for r in raw_replies
                    ]
                    time.sleep(0.4)
                formatted.append(msg_data)
            results.append({
                "channel": name,
                "count": len(msgs),
                "messages": formatted,
            })

        return results


def parse_time_range(args) -> tuple[float, float, str]:
    """Return (oldest_ts, latest_ts, label) from CLI args."""
    now = datetime.now(timezone.utc)
    today = now.replace(hour=0, minute=0, second=0, microsecond=0)

    def parse_dt(s):
        for fmt in ("%Y-%m-%d %H:%M:%S", "%Y-%m-%d %H:%M", "%Y-%m-%d"):
            try:
                return datetime.strptime(s, fmt).replace(tzinfo=timezone.utc)
            except ValueError:
                continue
        raise ValueError(f"Bad date: '{s}'")

    if args.from_date:
        o = parse_dt(args.from_date)
        l = parse_dt(args.to_date) if args.to_date else now
        return o.timestamp(), l.timestamp(), f"{args.from_date} -> {args.to_date or 'now'}"
    if args.yesterday:
        y = today - timedelta(days=1)
        return y.timestamp(), today.timestamp(), f"yesterday ({y:%Y-%m-%d})"
    if args.hours:
        o = now - timedelta(hours=args.hours)
        return o.timestamp(), now.timestamp(), f"last {args.hours}h"
    if args.days:
        o = now - timedelta(days=args.days)
        return o.timestamp(), now.timestamp(), f"last {args.days} days"
    if args.minutes:
        o = now - timedelta(minutes=args.minutes)
        return o.timestamp(), now.timestamp(), f"last {args.minutes} min"
    return today.timestamp(), now.timestamp(), "today"


def main():
    p = argparse.ArgumentParser(description="Slack Message Fetcher")

    t = p.add_mutually_exclusive_group()
    t.add_argument("--today", action="store_true", default=True)
    t.add_argument("--yesterday", action="store_true")
    t.add_argument("--hours", type=float, metavar="N")
    t.add_argument("--days", type=int, metavar="N")
    t.add_argument("--minutes", type=int, metavar="N")
    p.add_argument("--from", dest="from_date", metavar="DATE")
    p.add_argument("--to", dest="to_date", metavar="DATE")

    r = p.add_mutually_exclusive_group()
    r.add_argument("--unread", action="store_true", default=True)
    r.add_argument("--read", action="store_true")
    r.add_argument("--all", action="store_true")

    s = p.add_mutually_exclusive_group()
    s.add_argument("--dms-only", action="store_true")
    s.add_argument("--channels-only", action="store_true")
    p.add_argument("--channel", type=str, metavar="NAMES")

    p.add_argument("--mentions-me", action="store_true")
    p.add_argument("--mentions", type=str, metavar="NAMES")

    p.add_argument("--json", action="store_true")
    p.add_argument("--output", type=str, metavar="FILE")
    p.add_argument("--quiet", action="store_true")
    p.add_argument("--debug", action="store_true", help="Show raw message text for troubleshooting")
    p.add_argument("--with-threads", action="store_true", help="Fetch thread replies for threaded messages")

    args = p.parse_args()

    token = os.environ.get("SLACK_USER_TOKEN")
    if not token:
        sys.exit("Set SLACK_USER_TOKEN env var")

    fetcher = SlackFetcher(token)
    if not args.quiet:
        print(f"Authenticated as: {fetcher.auth['user']}\n", file=sys.stderr)

    oldest, latest, range_label = parse_time_range(args)
    read_filter = "all" if args.all else ("read" if args.read else "unread")

    types = []
    if not args.channels_only:
        types += ["im", "mpim"]
    if not args.dms_only:
        types += ["public_channel", "private_channel"]

    mention_patterns = set()
    if args.mentions_me:
        mention_patterns.add(f"<@{fetcher.my_id}")
    if args.mentions:
        mention_patterns |= fetcher.resolve_mentions(args.mentions.split(","))

    if not args.quiet:
        print(f"Time: {range_label} | Filter: {read_filter}" +
              (f" | Mentions: {mention_patterns}" if mention_patterns else ""), file=sys.stderr)
        print(file=sys.stderr)

    # NOTE: Comma-delimited split assumes channel names don't contain commas.
    # Slack channel names cannot contain commas, so this is safe.
    channel_filter = [n.strip() for n in args.channel.split(",")] if args.channel else None
    results = fetcher.fetch(oldest, latest, ",".join(types), read_filter, mention_patterns or None, channel_filter, debug=args.debug, with_threads=args.with_threads)

    if not results:
        if not args.quiet:
            print(f"No {read_filter} messages for {range_label}.", file=sys.stderr)
        if args.json:
            print("[]")
        return

    total = sum(r["count"] for r in results)

    if not args.quiet:
        print(f"\U0001f4ec {total} messages across {len(results)} conversations", file=sys.stderr)
        print("=" * 60, file=sys.stderr)
        for r in results:
            print(f"\n{r['channel']} ({r['count']})", file=sys.stderr)
            print("-" * 40, file=sys.stderr)
            for m in r["messages"]:
                t = datetime.fromisoformat(m["time"]).strftime("%H:%M")
                text = m["text"].replace("\n", "\n        ")
                print(f"  [{t}] {m['user']}: {text}", file=sys.stderr)
                if m.get("replies"):
                    for reply in m["replies"]:
                        rt = datetime.fromisoformat(reply["time"]).strftime("%H:%M")
                        rtext = reply["text"].replace("\n", "\n            ")
                        print(f"      [{rt}] {reply['user']}: {rtext}", file=sys.stderr)

    if args.json or args.output:
        data = json.dumps(results, indent=2, ensure_ascii=False)
        if args.json:
            print(data)
        if args.output:
            with open(args.output, "w") as f:
                f.write(data)
            if not args.quiet:
                print(f"\nSaved -> {args.output}", file=sys.stderr)


if __name__ == "__main__":
    main()
