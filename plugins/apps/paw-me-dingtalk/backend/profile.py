# -*- coding: utf-8 -*-
"""Bounded DingTalk owner-profile collection and projection."""

from __future__ import annotations

from typing import Any, Awaitable, Callable

from .dws import DwsClient


Progress = Callable[[str, int, str], Awaitable[None]]


def _rows(value: Any, keys: tuple[str, ...]) -> list[dict[str, Any]]:
    if isinstance(value, dict):
        for key in keys:
            candidate = value.get(key)
            if isinstance(candidate, list):
                return [row for row in candidate if isinstance(row, dict)]
        for child in value.values():
            found = _rows(child, keys)
            if found:
                return found
    return []


def _text(data: dict[str, Any], *keys: str) -> str:
    for key in keys:
        value = data.get(key)
        if isinstance(value, (str, int)) and str(value).strip():
            return str(value).strip()
    return ""


def _object(value: Any, keys: tuple[str, ...]) -> dict[str, Any]:
    if isinstance(value, dict):
        if any(key in value for key in keys):
            return value
        for child in value.values():
            found = _object(child, keys)
            if found:
                return found
    return {}


def _message_text(row: dict[str, Any]) -> str:
    content = row.get("content")
    if isinstance(content, str):
        return content.strip()
    if isinstance(content, dict):
        return _text(content, "text", "content")
    return ""


def _owner_message(row: dict[str, Any], owner_ids: set[str]) -> bool:
    explicit = row.get("isSelf", row.get("is_self"))
    if isinstance(explicit, bool):
        return explicit
    sender = row.get("sender")
    sender_data = sender if isinstance(sender, dict) else {}
    sender_id = _text(
        sender_data,
        "openDingTalkId",
        "openDingtalkId",
        "userId",
    ) or _text(
        row,
        "senderOpenDingTalkId",
        "senderOpenDingtalkId",
        "senderUserId",
    )
    return bool(sender_id and sender_id in owner_ids)


def _conversation_rows(payload: dict[str, Any]) -> list[dict[str, Any]]:
    return _rows(payload, ("conversations", "items", "list"))


def _conversation_id(row: dict[str, Any]) -> str:
    return _text(
        row,
        "openConversationId",
        "conversationId",
        "id",
    )


def _member_identity(row: dict[str, Any]) -> tuple[str, str]:
    member_id = _text(
        row,
        "openDingTalkId",
        "openDingtalkId",
        "memberOpenDingTalkId",
    )
    name = _text(row, "memberEmpName", "memberNick", "name")
    return member_id, name


class OwnerProfileCollector:
    """Collect a privacy-bounded snapshot outside the reply hot path."""

    def __init__(self, dws: DwsClient) -> None:
        self.dws = dws

    async def collect(
        self,
        *,
        corp_id: str,
        user_id: str,
        user_name: str,
        progress: Progress,
    ) -> tuple[dict[str, Any], list[str]]:
        """Return collected facts and non-fatal completeness errors."""
        errors: list[str] = []
        await progress("identity", 8, "正在读取本人组织身份")
        identity_payload = await self.dws.self_profile()
        identity = self._identity(
            identity_payload,
            corp_id,
            user_id,
            user_name,
        )
        owner_ids = {user_id}
        relations: dict[str, dict[str, Any]] = {}
        for dimension in ("supervisor", "subordinate"):
            try:
                payload = await self.dws.people_by_dimension(
                    identity.get("name") or user_name,
                    dimension,
                )
                self._add_formal_relations(relations, payload, dimension)
            except Exception as exc:  # noqa: BLE001
                errors.append(f"汇报关系 {dimension}: {exc}")

        await progress("conversations", 18, "正在读取近期会话目录")
        conversations = _conversation_rows(
            await self.dws.conversations(limit=20),
        )[:20]
        samples: list[dict[str, str]] = []
        for index, conversation in enumerate(conversations):
            conversation_id = _conversation_id(conversation)
            if not conversation_id:
                continue
            name = (
                _text(
                    conversation,
                    "conversationName",
                    "title",
                    "name",
                )
                or conversation_id
            )
            if index < 10:
                try:
                    members = await self.dws.group_members(conversation_id)
                    self._add_members(
                        relations,
                        members,
                        conversation_id,
                        owner_ids,
                        user_id,
                        user_name,
                    )
                except Exception:
                    pass
            try:
                history = await self.dws.conversation_history(
                    conversation_id,
                    limit=30,
                )
                self._add_history(
                    samples,
                    relations,
                    history,
                    conversation_id,
                    name,
                    owner_ids,
                )
            except Exception as exc:  # noqa: BLE001
                errors.append(f"{name}: {exc}")
            percent = 20 + int((index + 1) * 50 / max(len(conversations), 1))
            await progress(
                "conversations",
                percent,
                f"已检查 {index + 1}/{len(conversations)} 个近期会话",
            )
            if len(samples) >= 500:
                break

        await progress("work", 76, "正在读取待办与日程工作习惯")
        todo_rows: list[dict[str, Any]] = []
        calendar_rows: list[dict[str, Any]] = []
        try:
            todo_rows = _rows(
                await self.dws.created_todos(),
                ("created", "todos", "items", "list"),
            )[:100]
        except Exception as exc:  # noqa: BLE001
            errors.append(f"待办: {exc}")
        try:
            calendar_rows = _rows(
                await self.dws.agenda(days=30),
                ("events", "items", "list"),
            )[:100]
        except Exception as exc:  # noqa: BLE001
            errors.append(f"日程: {exc}")

        await progress("relationships", 90, "正在整理人物关系与来源")
        profile = {
            "identity": identity,
            "work_style": self._work_style(samples, todo_rows, calendar_rows),
            "relationships": sorted(
                relations.values(),
                key=lambda item: (
                    -int(item.get("interaction_count", 0)),
                    str(item.get("name", "")),
                ),
            )[:200],
            "coverage": {
                "conversation_count": len(conversations),
                "owner_message_count": len(samples[:500]),
                "todo_count": len(todo_rows),
                "calendar_count": len(calendar_rows),
                "peer_message_bodies_stored": False,
                "lookback_days": 30,
            },
        }
        await progress("saving", 97, "正在保存本地画像快照")
        return profile, errors[:20]

    @staticmethod
    def _add_formal_relations(
        relations: dict[str, dict[str, Any]],
        payload: dict[str, Any],
        dimension: str,
    ) -> None:
        kind = "manager" if dimension == "supervisor" else "subordinate"
        for row in _rows(payload, ("people", "persons", "items", "list")):
            person_id = _text(
                row,
                "openDingTalkId",
                "openDingtalkId",
                "userId",
            )
            if not person_id:
                continue
            name = _text(row, "name", "userName", "nick") or person_id
            relations[person_id] = {
                "subject_id": person_id,
                "name": name,
                "kinds": [kind],
                "shared_group_count": 0,
                "interaction_count": 0,
                "last_interaction_at": "",
                "confidence": "factual",
                "sources": [{"type": f"organization_{dimension}"}],
            }

    @staticmethod
    def _identity(
        payload: dict[str, Any],
        corp_id: str,
        user_id: str,
        user_name: str,
    ) -> dict[str, Any]:
        row = _object(payload, ("userId", "name", "title", "deptName"))
        departments = row.get("departments", row.get("deptName", []))
        if isinstance(departments, str):
            departments = [departments]
        if not isinstance(departments, list):
            departments = []
        labels = row.get("labels", row.get("roles", []))
        if isinstance(labels, str):
            labels = [labels]
        if not isinstance(labels, list):
            labels = []
        return {
            "corp_id": corp_id,
            "user_id": _text(row, "userId") or user_id,
            "name": _text(row, "name") or user_name,
            "title": _text(row, "title", "position"),
            "departments": [str(item) for item in departments[:20]],
            "roles": [str(item) for item in labels[:20]],
            "manager_user_id": _text(row, "managerUserId"),
            "source": "dingtalk_oauth_contact",
        }

    @staticmethod
    def _add_members(
        relations: dict[str, dict[str, Any]],
        payload: dict[str, Any],
        conversation_id: str,
        owner_ids: set[str],
        user_id: str,
        user_name: str,
    ) -> None:
        for row in _rows(payload, ("list", "members", "items")):
            member_id, name = _member_identity(row)
            member_user_id = _text(row, "userId", "memberUserId")
            if member_user_id == user_id or name == user_name:
                if member_id:
                    owner_ids.add(member_id)
                continue
            if not member_id:
                continue
            relation = relations.setdefault(
                member_id,
                {
                    "subject_id": member_id,
                    "name": name or member_id,
                    "kinds": ["shared_group"],
                    "shared_group_count": 0,
                    "interaction_count": 0,
                    "last_interaction_at": "",
                    "confidence": "observed",
                    "sources": [],
                },
            )
            relation["shared_group_count"] += 1
            relation["sources"].append(
                {
                    "type": "group_membership",
                    "conversation_id": conversation_id,
                },
            )

    @staticmethod
    def _add_history(
        samples: list[dict[str, str]],
        relations: dict[str, dict[str, Any]],
        payload: dict[str, Any],
        conversation_id: str,
        conversation_name: str,
        owner_ids: set[str],
    ) -> None:
        for row in _rows(payload, ("messages", "items", "list")):
            text = _message_text(row)
            if not text:
                continue
            created_at = _text(row, "createTime", "create_time")
            if _owner_message(row, owner_ids):
                if len(samples) < 500:
                    samples.append(
                        {
                            "text": text[:2000],
                            "created_at": created_at,
                        },
                    )
                continue
            sender = row.get("sender")
            sender_data = sender if isinstance(sender, dict) else {}
            sender_id = _text(
                sender_data,
                "openDingTalkId",
                "openDingtalkId",
            ) or _text(
                row,
                "senderOpenDingTalkId",
                "senderOpenDingtalkId",
            )
            if not sender_id:
                continue
            name = _text(sender_data, "name", "nick") or _text(row, "sender")
            relation = relations.setdefault(
                sender_id,
                {
                    "subject_id": sender_id,
                    "name": name or conversation_name,
                    "kinds": ["recent_collaboration"],
                    "shared_group_count": 0,
                    "interaction_count": 0,
                    "last_interaction_at": "",
                    "confidence": "observed",
                    "sources": [],
                },
            )
            relation["interaction_count"] += 1
            if created_at > relation["last_interaction_at"]:
                relation["last_interaction_at"] = created_at
            if not relation["sources"]:
                relation["sources"].append(
                    {
                        "type": "recent_conversation",
                        "conversation_id": conversation_id,
                    },
                )

    @staticmethod
    def _work_style(
        samples: list[dict[str, str]],
        todos: list[dict[str, Any]],
        events: list[dict[str, Any]],
    ) -> dict[str, Any]:
        texts = [row["text"] for row in samples[:500]]
        average = (
            round(sum(len(text) for text in texts) / len(texts), 1)
            if texts
            else 0
        )
        questions = sum("?" in text or "？" in text for text in texts)
        return {
            "message_count": len(texts),
            "average_message_length": average,
            "question_ratio": round(questions / len(texts), 2) if texts else 0,
            "voice_examples": texts[-40:],
            "created_todo_subjects": [
                _text(row, "subject", "title")
                for row in todos[:20]
                if _text(row, "subject", "title")
            ],
            "calendar_subjects": [
                _text(row, "summary", "subject", "title")
                for row in events[:20]
                if _text(row, "summary", "subject", "title")
            ],
        }


def profile_prompt(profile: dict[str, Any], subject_id: str) -> str:
    """Build compact instructions exclusively from the local snapshot."""
    identity = profile.get("identity", {})
    work = profile.get("work_style", {})
    relations = profile.get("relationships", [])
    current = next(
        (
            row
            for row in relations
            if isinstance(row, dict) and row.get("subject_id") == subject_id
        ),
        None,
    )
    examples = work.get("voice_examples", [])[-12:]
    lines = [
        f"本人：{identity.get('name') or '未命名'}",
        f"部门：{'、'.join(identity.get('departments', [])) or '未知'}",
        f"职位：{identity.get('title') or '未知'}",
        f"角色：{'、'.join(identity.get('roles', [])) or '未知'}",
        f"平均消息长度：{work.get('average_message_length', 0)} 字",
    ]
    if current:
        lines.append(
            "当前对话人物关系（仅作协作线索，不推断私人关系）："
            f"{current.get('name')}，互动 {current.get('interaction_count', 0)}"
            f" 次，共同群 {current.get('shared_group_count', 0)} 个。",
        )
    if examples:
        lines.append("本人近期表达样本：\n" + "\n".join(examples))
    return "\n".join(lines)
