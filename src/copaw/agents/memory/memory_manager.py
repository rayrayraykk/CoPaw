# -*- coding: utf-8 -*-
# pylint: disable=too-many-branches
"""Memory Manager for CoPaw agents.

Inherits from ReMeCopaw to provide memory management capabilities including:
- Message compaction and summarization
- Semantic memory search
- Memory file retrieval
- Tool result compaction
"""
import logging

from reme.reme_copaw import ReMeCopaw
from agentscope.formatter import FormatterBase
from agentscope.message import Msg
from agentscope.model import ChatModelBase
from agentscope.token import HuggingFaceTokenCounter
from agentscope.tool import Toolkit

from ...config.utils import load_config
from ...constant import MEMORY_COMPACT_RATIO

logger = logging.getLogger(__name__)


class MemoryManager(ReMeCopaw):
    """Memory manager that extends ReMeCopaw functionality for CoPaw agents.

    This class provides memory management capabilities including:
    - Memory compaction for long conversations
    - Semantic memory search using vector and full-text search
    - Memory file retrieval with pagination
    - Tool result compaction with file-based storage
    """

    def __init__(
        self,
        working_dir: str,
        chat_model: ChatModelBase,
        formatter: FormatterBase,
        token_counter: HuggingFaceTokenCounter,
        toolkit: Toolkit,
        max_input_length: int,
        memory_compact_ratio: float,
        vector_weight: float = 0.7,
        candidate_multiplier: float = 3.0,
        tool_result_threshold: int = 1000,
        retention_days: int = 7,
    ):
        """Initialize MemoryManager with ReMeCopaw configuration.

        Args:
            working_dir: Working directory path for memory storage
            chat_model: Language model for generating summaries
            formatter: Formatter for structuring model inputs/outputs
            token_counter: Token counting utility for length management
            toolkit: Collection of tools available to the application
            max_input_length: Maximum allowed input length in tokens
            memory_compact_ratio: Ratio at which to trigger compaction
                (0.0-1.0)
            vector_weight: Weight for vector search in hybrid search (0.0-1.0)
            candidate_multiplier: Multiplier for candidate retrieval in search
            tool_result_threshold: Size threshold for tool result compaction
            retention_days: Number of days to retain tool result files
        """

        global_config = load_config()
        language = "zh" if global_config.agents.language == "zh" else ""

        super().__init__(
            working_dir=working_dir,
            chat_model=chat_model,
            formatter=formatter,
            token_counter=token_counter,
            toolkit=toolkit,
            max_input_length=max_input_length,
            memory_compact_ratio=memory_compact_ratio,
            language=language,
            vector_weight=vector_weight,
            candidate_multiplier=candidate_multiplier,
            tool_result_threshold=tool_result_threshold,
            retention_days=retention_days,
        )

    def update_config_params(self):
        global_config = load_config()

        super().update_params(
            max_input_length=global_config.agents.running.max_input_length,
            memory_compact_ratio=MEMORY_COMPACT_RATIO,
            language=global_config.agents.language,
        )

    async def compact_memory(
        self,
        messages: list[Msg],
        previous_summary: str = "",
    ) -> str:
        self.update_config_params()
        return await super().compact_memory(
            messages=messages,
            previous_summary=previous_summary,
        )

    async def summary_memory(self, messages: list[Msg]) -> str:
        self.update_config_params()
        return await super().summary_memory(messages)
