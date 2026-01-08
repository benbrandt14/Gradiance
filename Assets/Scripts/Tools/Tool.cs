using UnityEngine;

namespace Tools
{
    public abstract class Tool : MonoBehaviour
    {
        public abstract string ToolName { get; }

        /// <summary>
        /// Called when the tool becomes the active tool.
        /// </summary>
        public virtual void OnToolSelected() { }

        /// <summary>
        /// Called when the tool is deselected.
        /// </summary>
        public virtual void OnToolDeselected() { }

        /// <summary>
        /// Called every frame (Update) while the tool is active.
        /// </summary>
        public virtual void OnToolUpdate() { }
    }
}
