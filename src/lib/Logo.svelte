<script lang="ts">
  import light from "../assets/logo-thanh-duy.svg";
  import dark from "../assets/logo-thanh-duy-dark.svg";

  let { size }: { size: number } = $props();
</script>

<!-- Logo vòng Enso, đổi bản sáng/tối theo giao diện đang chọn (data-theme trên <html>). -->
<span class="logo">
  <img class="light" src={light} alt="" width={size} height={size} />
  <img class="dark" src={dark} alt="" width={size} height={size} />
</span>

<style>
  .logo,
  img {
    display: block;
  }

  .dark,
  :global(:root[data-theme="dark"]) .light {
    display: none;
  }

  :global(:root[data-theme="dark"]) .dark {
    display: block;
  }
</style>
