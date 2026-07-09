<script>
  import { toast } from "../toastStore";
</script>

{#if $toast.visible}
  <div class="toast" class:error={$toast.type === "error"} class:success={$toast.type === "success"}>
    {$toast.message}
  </div>
{/if}

<style>
  .toast {
    position: fixed;
    bottom: 40px;
    left: 50%;
    transform: translateX(-50%);
    background: var(--ink);
    color: #fff;
    padding: 10px 20px;
    border-radius: 20px;
    font-size: 12.5px;
    box-shadow: var(--shadow-md);
    z-index: 200;
  }
  .toast.success {
    background: var(--success);
  }
  .toast.error {
    background: var(--danger);
  }
</style>
